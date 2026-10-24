//! Sign-in and the read/write policy (`[auth]` in `wirewolf.toml`).
//!
//! Two ways to sign in:
//!
//! - **OIDC**: the authorization code flow with PKCE against any issuer. `/auth/login` sends the
//!   browser to the issuer, `/auth/callback` checks the ID token, and the user is kept in an
//!   encrypted, HttpOnly, `SameSite=Lax` cookie. The key lives in `auth.session_key_file`.
//! - **Trusted header**: a reverse proxy that already signed the user in sets `Remote-User` (or
//!   whatever is configured). Only requests from `auth.trusted_header.proxies` are believed.
//!
//! [`policy`] guards `/api/v1`. Reads are open unless `require_for_reads`. Writes (anything but
//! `GET`/`HEAD`) need a user when a sign-in method is configured, and are refused with none
//! configured unless `allow_unauthenticated_writes`. Cookie sessions must also send the session's
//! CSRF token as `X-Wirewolf-CSRF`, and cross-site writes are always refused.

use std::net::{IpAddr, SocketAddr};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use axum::extract::connect_info::MockConnectInfo;
use axum::extract::{ConnectInfo, Query, Request, State};
use axum::http::{HeaderMap, Method, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Redirect, Response};
use axum::routing::{get, post};
use axum::{Extension, Json, Router};
use axum_extra::extract::cookie::{Cookie, Key, PrivateCookieJar, SameSite};
use base64::Engine;
use ipnet::IpNet;
use openidconnect::core::{CoreAuthenticationFlow, CoreClient, CoreProviderMetadata};
use openidconnect::{
    AuthorizationCode, OAuth2TokenResponse, ClientId, ClientSecret, CsrfToken, IssuerUrl, Nonce, PkceCodeChallenge,
    PkceCodeVerifier, RedirectUrl, Scope, TokenResponse,
};
use serde::{Deserialize, Serialize};
use tokio::sync::OnceCell;
use wirewolf_core::config;
use wirewolf_core::model::{Session, SessionUser};

pub const CSRF_HEADER: &str = "x-wirewolf-csrf";
const SESSION_COOKIE: &str = "wirewolf_session";
const LOGIN_COOKIE: &str = "wirewolf_login";

/// The configured sign-in methods and policy, ready to serve.
pub struct AuthState {
    config: config::Auth,
    oidc: Option<Oidc>,
    proxies: Vec<IpNet>,
    key: Key,
    secure_cookies: bool,
    /// `server.allowed_hosts` plus the host in `auth.redirect_url`.
    hosts: Vec<String>,
}

struct Oidc {
    client_id: String,
    client_secret: String,
    redirect_url: RedirectUrl,
    issuer: IssuerUrl,
    http: openidconnect::reqwest::Client,
    /// Discovered on the first sign-in, so the server starts even if the issuer is down.
    metadata: OnceCell<CoreProviderMetadata>,
}

impl AuthState {
    /// No sign-in, and anyone may write. For the demo and tests.
    pub fn open() -> Self {
        let config = config::Auth { allow_unauthenticated_writes: true, ..config::Auth::default() };
        Self { config, oidc: None, proxies: vec![], key: Key::generate(), secure_cookies: false, hosts: vec![] }
    }

    /// Builds the state from `[auth]`: reads the client secret and the session key (creating the
    /// key file if it's missing).
    pub fn from_config(config: &config::Auth) -> Result<Self, String> {
        let mut config = config.clone();
        let proxies = config
            .trusted_header
            .proxies
            .iter()
            .map(|p| p.parse::<IpNet>().or_else(|_| p.parse::<IpAddr>().map(IpNet::from)))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("auth.trusted_header.proxies: {e}"))?;
        if config.header_enabled() && proxies.is_empty() {
            return Err("auth.trusted_header.user is set, so auth.trusted_header.proxies must list the proxy".into());
        }
        let mut key = Key::generate();
        let mut oidc = None;
        if config.oidc_enabled() {
            if config.client_secret.is_empty() {
                config.client_secret = std::env::var("WIREWOLF_AUTH_CLIENT_SECRET").unwrap_or_default();
            }
            if config.client_secret.is_empty() && !config.client_secret_file.is_empty() {
                let path = &config.client_secret_file;
                config.client_secret = std::fs::read_to_string(path).map_err(|e| format!("auth.client_secret_file {path}: {e}"))?.trim().to_owned();
            }
            if config.allowed_emails.is_empty() && config.allowed_groups.is_empty() && !config.allow_any_user {
                return Err("auth.issuer is set but auth.allowed_emails and auth.allowed_groups are empty, so anyone with an account at the issuer could change settings. List who may sign in, or set auth.allow_any_user = true".into());
            }
            for (key, value) in [("client_id", &config.client_id), ("redirect_url", &config.redirect_url)] {
                if value.is_empty() {
                    return Err(format!("auth.issuer is set, so auth.{key} is required"));
                }
            }
            let http = openidconnect::reqwest::ClientBuilder::new()
                // Following redirects here would open the door to SSRF.
                .redirect(openidconnect::reqwest::redirect::Policy::none())
                .timeout(Duration::from_secs(15))
                .build()
                .map_err(|e| e.to_string())?;
            oidc = Some(Oidc {
                client_id: config.client_id.clone(),
                client_secret: config.client_secret.clone(),
                redirect_url: RedirectUrl::new(config.redirect_url.clone()).map_err(|e| format!("auth.redirect_url: {e}"))?,
                issuer: IssuerUrl::new(config.issuer.clone()).map_err(|e| format!("auth.issuer: {e}"))?,
                http,
                metadata: OnceCell::new(),
            });
            key = load_key(&config.session_key_file)?;
        }
        let secure_cookies = config.redirect_url.starts_with("https://");
        let hosts = openidconnect::url::Url::parse(&config.redirect_url).ok().and_then(|u| u.host_str().map(str::to_owned)).into_iter().collect();
        Ok(Self { config, oidc, proxies, key, secure_cookies, hosts })
    }

    /// Also answer to these host names (`server.allowed_hosts`). See [`host_guard`].
    pub fn with_allowed_hosts(mut self, hosts: &[String]) -> Self {
        self.hosts.extend(hosts.iter().map(|h| h.to_ascii_lowercase()));
        self
    }

    /// Whether a `Host` header names this server. IP addresses and local names always do; a
    /// public DNS name only if it's listed. A page whose own name is rebound to this server's
    /// address sends its own name, so it's refused.
    fn host_allowed(&self, host: &str) -> bool {
        let host = match host.rsplit_once(':') {
            Some((h, port)) if port.chars().all(|c| c.is_ascii_digit()) && !h.ends_with(':') => h,
            _ => host,
        };
        let host = host.trim_start_matches('[').trim_end_matches(']').trim_end_matches('.').to_ascii_lowercase();
        const LOCAL: &[&str] = &[".local", ".lan", ".home", ".home.arpa", ".internal", ".localdomain", ".localhost", ".ts.net"];
        host.parse::<IpAddr>().is_ok()
            || !host.contains('.')
            || LOCAL.iter().any(|s| host.ends_with(s))
            || self.hosts.iter().any(|h| h == "*" || *h == host || h.strip_prefix('*').is_some_and(|suffix| suffix.starts_with('.') && host.ends_with(suffix)))
    }


    fn configured(&self) -> bool {
        self.oidc.is_some() || self.config.header_enabled()
    }

    fn allowed(&self, email: Option<&str>, groups: &[String]) -> bool {
        let c = &self.config;
        if c.allowed_emails.is_empty() && c.allowed_groups.is_empty() {
            return true;
        }
        email.is_some_and(|e| c.allowed_emails.iter().any(|a| a.eq_ignore_ascii_case(e)))
            || groups.iter().any(|g| c.allowed_groups.contains(g))
    }

    /// Who made this request, if anyone.
    fn identify(&self, headers: &HeaderMap, peer: Option<IpAddr>) -> Viewer {
        let th = &self.config.trusted_header;
        if self.config.header_enabled() && peer.is_some_and(|ip| self.proxies.iter().any(|n| n.contains(&ip))) {
            let get = |name: &str| (!name.is_empty()).then(|| headers.get(name)?.to_str().ok()).flatten().filter(|v| !v.is_empty());
            if let Some(user) = get(&th.user) {
                let email = get(&th.email);
                let groups: Vec<String> = get(&th.groups).unwrap_or("").split(',').map(|g| g.trim().to_owned()).filter(|g| !g.is_empty()).collect();
                if self.allowed(email.or(Some(user)), &groups) {
                    let user = SessionUser { id: user.into(), name: get(&th.name).map(Into::into), email: email.map(Into::into), via: "header".into() };
                    return Viewer { user: Some(user), csrf: None };
                }
            }
        }
        if self.oidc.is_some() {
            let jar = PrivateCookieJar::from_headers(headers, self.key.clone());
            if let Some(stored) = jar.get(SESSION_COOKIE).and_then(|c| serde_json::from_str::<Stored>(c.value()).ok())
                && stored.exp > jiff::Timestamp::now().as_second()
                // Someone taken off the allowlist is signed out at the next restart.
                && self.allowed(stored.email.as_deref(), &stored.groups)
            {
                let user = SessionUser { id: stored.id, name: stored.name, email: stored.email, via: "oidc".into() };
                return Viewer { user: Some(user), csrf: Some(stored.csrf) };
            }
        }
        Viewer::default()
    }

    fn can_write(&self, viewer: &Viewer) -> bool {
        if self.configured() { viewer.user.is_some() } else { self.config.allow_unauthenticated_writes }
    }

    fn cookie<'a>(&self, name: &'a str, value: String, path: &'a str, max_age: Duration) -> Cookie<'a> {
        Cookie::build((name, value))
            .path(path)
            .http_only(true)
            .secure(self.secure_cookies)
            .same_site(SameSite::Lax)
            .max_age(time::Duration::seconds(max_age.as_secs() as i64))
            .build()
    }
}

/// The session key: 64 random bytes, created with mode 0600 if missing.
fn load_key(path: &Path) -> Result<Key, String> {
    match std::fs::read(path) {
        Ok(bytes) => Key::try_from(bytes.as_slice()).map_err(|e| format!("auth.session_key_file {}: {e}", path.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            let key = Key::generate();
            let mut options = std::fs::OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
            let write = |mut f: std::fs::File| std::io::Write::write_all(&mut f, key.master());
            options.open(path).and_then(write).map_err(|e| format!("can't create auth.session_key_file {}: {e}", path.display()))?;
            tracing::info!("created session key {}", path.display());
            Ok(key)
        }
        Err(e) => Err(format!("auth.session_key_file {}: {e}", path.display())),
    }
}

/// The signed-in user as kept in the session cookie.
#[derive(Serialize, Deserialize)]
struct Stored {
    id: String,
    name: Option<String>,
    email: Option<String>,
    /// Kept so the allowlist is checked on every request, not just at sign-in.
    #[serde(default)]
    groups: Vec<String>,
    csrf: String,
    exp: i64,
}

/// A sign-in in progress, between `/auth/login` and `/auth/callback`.
#[derive(Serialize, Deserialize)]
struct Pending {
    state: String,
    nonce: String,
    verifier: String,
    return_to: String,
}

/// What the request's viewer may do, for handlers that show more to writers.
#[derive(Clone, Copy, Debug, Default)]
pub struct Access {
    pub can_write: bool,
}

#[derive(Clone, Default)]
struct Viewer {
    user: Option<SessionUser>,
    csrf: Option<String>,
}

/// Middleware for `/api/v1`: decides whether this request may read or write.
pub async fn policy(State(auth): State<Arc<AuthState>>, mut req: Request, next: Next) -> Response {
    let ext = req.extensions();
    let peer = ext.get::<ConnectInfo<SocketAddr>>().map(|c| c.0).or_else(|| ext.get::<MockConnectInfo<SocketAddr>>().map(|c| c.0)).map(|a| a.ip());
    let viewer = auth.identify(req.headers(), peer);
    req.extensions_mut().insert(Access { can_write: auth.can_write(&viewer) });
    let write = !matches!(*req.method(), Method::GET | Method::HEAD | Method::OPTIONS);
    // Exactly `/session` (the path inside the `/api/v1` nest), so no other route skips the read check.
    if req.uri().path() == "/session" && !write {
        req.extensions_mut().insert(viewer);
        return next.run(req).await;
    }
    if write {
        if cross_site(req.headers()) {
            return (StatusCode::FORBIDDEN, "cross-site request refused").into_response();
        }
        if viewer.user.is_none() {
            return if auth.configured() {
                (StatusCode::UNAUTHORIZED, "sign in to do this").into_response()
            } else if auth.config.allow_unauthenticated_writes {
                next.run(req).await
            } else {
                (StatusCode::FORBIDDEN, "writes are off: configure [auth] or set auth.allow_unauthenticated_writes").into_response()
            };
        }
        if let Some(csrf) = &viewer.csrf {
            let sent = req.headers().get(CSRF_HEADER).and_then(|v| v.to_str().ok());
            if sent != Some(csrf.as_str()) {
                return (StatusCode::FORBIDDEN, "missing or wrong X-Wirewolf-CSRF").into_response();
            }
        }
    } else if auth.config.require_for_reads && auth.configured() && viewer.user.is_none() {
        return (StatusCode::UNAUTHORIZED, "sign in to see this").into_response();
    }
    next.run(req).await
}

/// Middleware for every route: refuses requests for host names this server doesn't answer to
/// (DNS rebinding). Requests with no `Host` header aren't from a browser and pass.
pub async fn host_guard(State(auth): State<Arc<AuthState>>, req: Request, next: Next) -> Response {
    let host = req.headers().get(axum::http::header::HOST).and_then(|v| v.to_str().ok()).or_else(|| req.uri().host());
    match host {
        Some(h) if !auth.host_allowed(h) => {
            tracing::warn!("refused a request for host {h:?}; add it to server.allowed_hosts if it's yours");
            (StatusCode::MISDIRECTED_REQUEST, format!("wirewolf doesn't answer to {h:?}. If that name is yours, add it to server.allowed_hosts.")).into_response()
        }
        _ => next.run(req).await,
    }
}

/// Browsers mark cross-site requests; refuse those even when cookies would allow them.
fn cross_site(headers: &HeaderMap) -> bool {
    let get = |name| headers.get(name).and_then(|v| v.to_str().ok());
    if get("sec-fetch-site") == Some("cross-site") {
        return true;
    }
    let Some(origin) = get("origin") else { return false };
    let origin_host = origin.split_once("://").map_or(origin, |(_, rest)| rest);
    ![get("x-forwarded-host"), get("host")].into_iter().flatten().any(|h| h.eq_ignore_ascii_case(origin_host))
}

/// `GET /api/v1/session`, mounted inside `/api/v1`.
pub fn api_routes<S: Clone + Send + Sync + 'static>() -> Router<S> {
    Router::new().route("/session", get(session))
}

/// `/auth/login`, `/auth/callback`, `/auth/logout`.
pub fn routes<S: Clone + Send + Sync + 'static>() -> Router<S> {
    Router::new().route("/auth/login", get(login)).route("/auth/callback", get(callback)).route("/auth/logout", post(logout))
}

async fn session(Extension(auth): Extension<Arc<AuthState>>, Extension(viewer): Extension<Viewer>) -> Json<Session> {
    let mut methods = vec![];
    if auth.oidc.is_some() {
        methods.push("oidc".to_owned());
    }
    if auth.config.header_enabled() {
        methods.push("header".to_owned());
    }
    let oidc_user = viewer.csrf.is_some();
    Json(Session {
        methods,
        can_write: auth.can_write(&viewer),
        reads_require_login: auth.config.require_for_reads && auth.configured(),
        csrf: viewer.csrf.clone(),
        login_url: auth.oidc.as_ref().map(|_| "/auth/login".to_owned()),
        logout_url: oidc_user.then(|| "/auth/logout".to_owned()),
        user: viewer.user,
    })
}

fn fail(status: StatusCode, message: impl std::fmt::Display) -> Response {
    tracing::warn!("sign-in failed: {message}");
    (status, format!("Sign-in failed: {message}")).into_response()
}

impl Oidc {
    async fn client(&self) -> Result<CoreClient<openidconnect::EndpointSet, openidconnect::EndpointNotSet, openidconnect::EndpointNotSet, openidconnect::EndpointNotSet, openidconnect::EndpointMaybeSet, openidconnect::EndpointMaybeSet>, String> {
        let metadata = self
            .metadata
            .get_or_try_init(|| CoreProviderMetadata::discover_async(self.issuer.clone(), &self.http))
            .await
            .map_err(|e| format!("discovery at {}: {e}", self.issuer.as_str()))?;
        Ok(CoreClient::from_provider_metadata(metadata.clone(), ClientId::new(self.client_id.clone()), Some(ClientSecret::new(self.client_secret.clone())))
            .set_redirect_uri(self.redirect_url.clone()))
    }

    /// Groups from the userinfo endpoint, for issuers that leave them out of the ID token. The
    /// answer must be about the same subject as the ID token.
    async fn userinfo_groups(&self, access_token: &str, subject: &str) -> Result<Vec<String>, String> {
        let metadata = self.metadata.get().ok_or("no discovery yet")?;
        let Some(url) = metadata.userinfo_endpoint() else { return Ok(vec![]) };
        let res = self.http.get(url.url().clone()).bearer_auth(access_token).send().await.map_err(|e| e.to_string())?;
        let res = res.error_for_status().map_err(|e| e.to_string())?;
        let value: serde_json::Value = serde_json::from_slice(&res.bytes().await.map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
        if value.get("sub").and_then(|s| s.as_str()) != Some(subject) {
            return Err("userinfo is about a different subject".into());
        }
        Ok(groups_in(&value))
    }
}

#[derive(Deserialize)]
struct LoginQuery {
    return_to: Option<String>,
}

async fn login(Extension(auth): Extension<Arc<AuthState>>, headers: HeaderMap, Query(q): Query<LoginQuery>) -> Response {
    let Some(oidc) = &auth.oidc else { return StatusCode::NOT_FOUND.into_response() };
    let client = match oidc.client().await {
        Ok(c) => c,
        Err(e) => return fail(StatusCode::BAD_GATEWAY, e),
    };
    let (challenge, verifier) = PkceCodeChallenge::new_random_sha256();
    let mut request = client.authorize_url(CoreAuthenticationFlow::AuthorizationCode, CsrfToken::new_random, Nonce::new_random);
    for scope in &auth.config.scopes {
        request = request.add_scope(Scope::new(scope.clone()));
    }
    let (url, state, nonce) = request.set_pkce_challenge(challenge).url();
    // Only same-site paths, so this can't be used as an open redirect.
    let return_to = q.return_to.filter(|r| r.starts_with('/') && !r.starts_with("//") && !r.starts_with("/\\")).unwrap_or_else(|| "/".into());
    let pending = Pending { state: state.secret().clone(), nonce: nonce.secret().clone(), verifier: verifier.secret().clone(), return_to };
    let value = serde_json::to_string(&pending).expect("serializes");
    let jar = PrivateCookieJar::from_headers(&headers, auth.key.clone()).add(auth.cookie(LOGIN_COOKIE, value, "/auth", Duration::from_secs(600)));
    (jar, Redirect::to(url.as_str())).into_response()
}

#[derive(Deserialize)]
struct CallbackQuery {
    code: Option<String>,
    state: Option<String>,
    error: Option<String>,
    error_description: Option<String>,
}

async fn callback(Extension(auth): Extension<Arc<AuthState>>, headers: HeaderMap, Query(q): Query<CallbackQuery>) -> Response {
    let Some(oidc) = &auth.oidc else { return StatusCode::NOT_FOUND.into_response() };
    if let Some(error) = q.error {
        return fail(StatusCode::UNAUTHORIZED, format!("{error} {}", q.error_description.unwrap_or_default()));
    }
    let jar = PrivateCookieJar::from_headers(&headers, auth.key.clone());
    let Some(pending) = jar.get(LOGIN_COOKIE).and_then(|c| serde_json::from_str::<Pending>(c.value()).ok()) else {
        return fail(StatusCode::BAD_REQUEST, "no sign-in in progress (expired, or cookies are blocked)");
    };
    if q.state.as_deref() != Some(pending.state.as_str()) {
        return fail(StatusCode::BAD_REQUEST, "state mismatch");
    }
    let Some(code) = q.code else { return fail(StatusCode::BAD_REQUEST, "no code") };
    let client = match oidc.client().await {
        Ok(c) => c,
        Err(e) => return fail(StatusCode::BAD_GATEWAY, e),
    };
    let exchange = match client.exchange_code(AuthorizationCode::new(code)) {
        Ok(x) => x,
        Err(e) => return fail(StatusCode::BAD_GATEWAY, e),
    };
    let token = match exchange.set_pkce_verifier(PkceCodeVerifier::new(pending.verifier)).request_async(&oidc.http).await {
        Ok(t) => t,
        Err(e) => return fail(StatusCode::BAD_GATEWAY, format!("token exchange: {e}")),
    };
    let Some(id_token) = token.id_token() else { return fail(StatusCode::BAD_GATEWAY, "issuer sent no ID token") };
    let claims = match id_token.claims(&client.id_token_verifier(), &Nonce::new(pending.nonce)) {
        Ok(c) => c,
        Err(e) => return fail(StatusCode::UNAUTHORIZED, format!("ID token: {e}")),
    };
    // Only an email the issuer says it verified. Some issuers leave the claim out and let users
    // set any address, which would let them claim one in auth.allowed_emails.
    let email = claims.email().filter(|_| claims.email_verified() == Some(true)).map(|e| e.as_str().to_owned());
    let mut groups = groups_claim(&id_token.to_string());
    if groups.is_empty() && !auth.config.allowed_groups.is_empty() {
        groups = oidc.userinfo_groups(token.access_token().secret(), claims.subject().as_str()).await.unwrap_or_else(|e| {
            tracing::warn!("couldn't read groups from userinfo: {e}");
            vec![]
        });
    }
    if !auth.allowed(email.as_deref(), &groups) {
        return fail(StatusCode::FORBIDDEN, format!("{} is not in auth.allowed_emails or auth.allowed_groups", email.as_deref().unwrap_or(claims.subject().as_str())));
    }
    let name = claims
        .name()
        .and_then(|n| n.get(None))
        .map(|n| n.as_str().to_owned())
        .or_else(|| claims.preferred_username().map(|u| u.as_str().to_owned()));
    let ttl = auth.config.session_ttl.0;
    let stored = Stored {
        id: claims.subject().as_str().to_owned(),
        name,
        email,
        // Only the groups that matter here, so a user in hundreds doesn't overflow the cookie.
        groups: groups.into_iter().filter(|g| auth.config.allowed_groups.contains(g)).collect(),
        csrf: CsrfToken::new_random().secret().clone(),
        exp: jiff::Timestamp::now().as_second() + ttl.as_secs() as i64,
    };
    tracing::info!("signed in {}", stored.email.as_deref().unwrap_or(&stored.id));
    let jar = jar
        .remove(Cookie::build(LOGIN_COOKIE).path("/auth"))
        .add(auth.cookie(SESSION_COOKIE, serde_json::to_string(&stored).expect("serializes"), "/", ttl));
    (jar, Redirect::to(&pending.return_to)).into_response()
}

/// The `groups` claim, read from an ID token whose signature was already checked.
fn groups_claim(jwt: &str) -> Vec<String> {
    let payload = jwt.split('.').nth(1).unwrap_or("");
    let Ok(bytes) = base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(payload) else { return vec![] };
    let Ok(value) = serde_json::from_slice::<serde_json::Value>(&bytes) else { return vec![] };
    groups_in(&value)
}

/// The `groups` claim of an ID token payload or userinfo response: a list, or a single name.
fn groups_in(value: &serde_json::Value) -> Vec<String> {
    match value.get("groups") {
        Some(serde_json::Value::Array(a)) => a.iter().filter_map(|g| g.as_str().map(Into::into)).collect(),
        Some(serde_json::Value::String(s)) => vec![s.clone()],
        _ => vec![],
    }
}

async fn logout(Extension(auth): Extension<Arc<AuthState>>, headers: HeaderMap) -> Response {
    let viewer = auth.identify(&headers, None);
    let sent = headers.get(CSRF_HEADER).and_then(|v| v.to_str().ok());
    if viewer.csrf.is_none() || sent != viewer.csrf.as_deref() {
        return StatusCode::FORBIDDEN.into_response();
    }
    let jar = PrivateCookieJar::from_headers(&headers, auth.key.clone()).remove(Cookie::build(SESSION_COOKIE).path("/"));
    (jar, StatusCode::NO_CONTENT).into_response()
}
