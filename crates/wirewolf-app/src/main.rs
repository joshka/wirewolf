fn main() {
    if let Err(e) = wirewolf_app::run() {
        eprintln!("wirewolf: {e:#}");
        std::process::exit(1);
    }
}
