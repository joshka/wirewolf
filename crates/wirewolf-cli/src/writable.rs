//! Checks at startup that the files `serve` writes are writable, so a volume left owned by
//! another user (say, root from an earlier run) fails once with the path and the ids to fix,
//! instead of crash-looping on a bare "permission denied" from deep inside SQLite.

use std::fs::OpenOptions;
use std::io;
use std::path::{Path, PathBuf};

/// What a path is for, and whether it must already exist to be useful.
pub struct Need {
    pub what: &'static str,
    pub path: PathBuf,
    /// An existing file that is only read (the session key) needs no write access.
    pub read_only_if_present: bool,
}

/// Returns one message per path that can't be written, naming the path, its owner, and the
/// user and group this process runs as.
pub fn check(needs: &[Need]) -> Result<(), String> {
    let problems: Vec<String> = needs.iter().filter_map(|n| problem(n).map(|e| describe(n, &e))).collect();
    if problems.is_empty() {
        return Ok(());
    }
    let (uid, gid) = ids();
    Err(format!(
        "{}\nwirewolf runs as uid {uid} gid {gid}; give it the files, e.g. `chown -R {uid}:{gid} <data dir>`",
        problems.join("\n")
    ))
}

/// The path and the error that makes it unusable, or `None` if it's fine.
fn problem(need: &Need) -> Option<(PathBuf, io::Error)> {
    let path = &need.path;
    if path.exists() {
        let open = if need.read_only_if_present { OpenOptions::new().read(true).open(path) } else { OpenOptions::new().append(true).open(path) };
        if let Err(e) = open {
            return Some((path.clone(), e));
        }
        if need.read_only_if_present {
            return None;
        }
    }
    // A missing file is created, and SQLite and the config editor also write files beside it.
    let dir = path.parent().filter(|d| !d.as_os_str().is_empty()).unwrap_or(Path::new("."));
    if !dir.exists() {
        // Created on demand; look at the nearest ancestor that exists.
        let existing = dir.ancestors().find(|a| a.exists()).unwrap_or(Path::new("."));
        return dir_problem(existing);
    }
    dir_problem(dir)
}

fn dir_problem(dir: &Path) -> Option<(PathBuf, io::Error)> {
    let probe = dir.join(format!(".wirewolf-write-check-{}", std::process::id()));
    match OpenOptions::new().write(true).create_new(true).open(&probe) {
        Ok(_) => {
            let _ = std::fs::remove_file(&probe);
            None
        }
        Err(e) => Some((dir.to_path_buf(), e)),
    }
}

fn describe(need: &Need, (path, err): &(PathBuf, io::Error)) -> String {
    let owner = owner(path).map(|(u, g, mode)| format!(", owned by uid {u} gid {g} mode {mode:o}")).unwrap_or_default();
    let what = need.what;
    if *path == need.path {
        format!("{what} {} isn't usable{owner}: {err}", path.display())
    } else {
        format!("{what} {}: can't write to {}{owner}: {err}", need.path.display(), path.display())
    }
}

#[cfg(unix)]
fn owner(path: &Path) -> Option<(u32, u32, u32)> {
    use std::os::unix::fs::MetadataExt;
    std::fs::metadata(path).ok().map(|m| (m.uid(), m.gid(), m.mode() & 0o7777))
}

#[cfg(not(unix))]
fn owner(_: &Path) -> Option<(u32, u32, u32)> {
    None
}

#[cfg(unix)]
fn ids() -> (u32, u32) {
    // SAFETY: geteuid and getegid can't fail.
    unsafe { (libc::geteuid(), libc::getegid()) }
}

#[cfg(not(unix))]
fn ids() -> (u32, u32) {
    (0, 0)
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn need(what: &'static str, path: PathBuf) -> Need {
        Need { what, path, read_only_if_present: false }
    }

    fn tempdir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("wirewolf-writable-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn writable_files_and_missing_files_pass() {
        let dir = tempdir("ok");
        std::fs::write(dir.join("wirewolf.db"), "").unwrap();
        assert!(check(&[need("storage.db", dir.join("wirewolf.db")), need("config", dir.join("sub/wirewolf.toml"))]).is_ok());
    }

    #[test]
    fn names_the_read_only_file_and_ids() {
        // Root ignores permission bits, so this test means nothing there.
        if ids().0 == 0 {
            return;
        }
        let dir = tempdir("ro");
        let db = dir.join("wirewolf.db");
        std::fs::write(&db, "").unwrap();
        std::fs::set_permissions(&db, std::fs::Permissions::from_mode(0o444)).unwrap();
        let err = check(&[need("storage.db", db.clone())]).unwrap_err();
        assert!(err.contains(&format!("storage.db {} isn't usable", db.display())), "{err}");
        assert!(err.contains("mode 444"), "{err}");
        assert!(err.contains(&format!("runs as uid {}", ids().0)), "{err}");
    }

    #[test]
    fn names_the_read_only_dir_for_a_new_file() {
        if ids().0 == 0 {
            return;
        }
        let dir = tempdir("rodir");
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o555)).unwrap();
        let key = dir.join("wirewolf-session.key");
        let result = check(&[Need { what: "auth.session_key_file", path: key, read_only_if_present: true }]);
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755)).unwrap();
        let err = result.unwrap_err();
        assert!(err.contains(&format!("can't write to {}", dir.display())), "{err}");
    }
}
