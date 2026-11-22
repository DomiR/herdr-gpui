//! The dashboard's routes, for any relay that is a [`Site`]. They change
//! nothing on the relay, and are served only when its operator set a token.
//!
//! - `GET /dashboard`: the page, or a redirect to the login.
//! - `GET`/`POST /dashboard/login`, `POST /dashboard/logout`. Three wrong
//!   tokens block the address for 15 minutes ([`lockout`](crate::lockout)).
//! - `GET /dashboard/api/live`: a snapshot a second, as Server-Sent Events.
//! - `GET /dashboard/api/stats`: one snapshot.

use crate::{
    auth::{self, Account},
    limit::{Limiter, Rate},
    live::{self, Streams},
    lockout::{Lockout, Verdict},
    metrics::Login,
};
use axum::{
    Json, Router,
    body::Bytes,
    extract::{ConnectInfo, DefaultBodyLimit, RawQuery, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response, sse::Sse},
    routing::{get, post},
};
use serde::Serialize;
use std::{
    net::{IpAddr, SocketAddr},
    sync::{Arc, Mutex},
    time::{Instant, SystemTime, UNIX_EPOCH},
};

const LOGIN: &str = include_str!("assets/login.html");
const STYLE: &str = include_str!("assets/style.css");
const ICON: &str = include_str!("assets/icon.svg");
const FAILED: &str = r#"<p class="error" role="alert">That token is not right. After 3 wrong tries, this address is blocked for 15 minutes.</p>"#;
const BLOCKED: &str = r#"<p class="error" role="alert">Too many wrong tokens from this address. It is blocked for 15 minutes.</p>"#;
const LIMITED: &str = r#"<p class="error" role="alert">Too many sign-in attempts right now. Wait a minute and try again.</p>"#;
const MESSAGE_SLOT: &str = "<!--message-->";
const NAME_SLOT: &str = "__NAME__";
const MAX_FORM: usize = 2 * 1024;
const CSP: &str = "default-src 'none'; script-src 'self'; style-src 'self'; connect-src 'self'; \
img-src 'self'; form-action 'self'; frame-ancestors 'none'; base-uri 'none'";

/// What a relay gives its dashboard. Each relay implements it once, over
/// the state its own routes share.
pub trait Site: Clone + Send + Sync + 'static {
    /// What the page draws, as JSON.
    type Stats: Serialize;

    /// The address `peer`'s request came from, keyed as the relay's own
    /// limits key it: behind a proxy, the one the proxy reports.
    fn address(&self, peer: SocketAddr, headers: &HeaderMap) -> IpAddr;

    /// Counts a login's outcome.
    fn login(&self, outcome: Login);

    /// The numbers now.
    fn stats(&self) -> Self::Stats;
}

/// A relay's own page: its name, as the login shows it, its HTML, and the
/// script that draws its stats. The style sheet and icon are shared.
#[derive(Clone, Copy, Debug)]
pub struct Page {
    pub index: &'static str,
    pub name: &'static str,
    pub script: &'static str,
}

/// What the dashboard routes share.
#[derive(Clone, Debug)]
struct Board<S> {
    account: Arc<Account>,
    /// Logins from all addresses, so many addresses cannot guess faster.
    everyone: Arc<Mutex<Limiter<()>>>,
    live: Arc<Streams>,
    lockout: Arc<Lockout>,
    page: Page,
    site: S,
    /// Where `everyone`'s milliseconds count from: a monotonic clock, so a
    /// wall clock stepping back cannot stop its refills.
    started: Instant,
}

/// The dashboard's routes for `site`, signed in to with `account`.
pub fn router<S: Site>(site: S, account: Arc<Account>, page: Page) -> Router {
    let board = Board {
        account,
        everyone: Arc::new(Mutex::new(Limiter::new(Rate {
            burst: 30,
            per_minute: 60,
        }))),
        live: Arc::default(),
        lockout: Arc::default(),
        page,
        site,
        started: Instant::now(),
    };
    let script = page.script;
    Router::new()
        .route("/dashboard", get(index::<S>))
        .route("/dashboard/login", get(login_page::<S>).post(login::<S>))
        .route("/dashboard/logout", post(logout::<S>))
        .route("/dashboard/api/live", get(live_stats::<S>))
        .route("/dashboard/api/stats", get(stats::<S>))
        .route(
            "/dashboard/app.js",
            get(move || async move { asset(script, "text/javascript; charset=utf-8") }),
        )
        .route(
            "/dashboard/style.css",
            get(|| async { asset(STYLE, "text/css; charset=utf-8") }),
        )
        .route(
            "/dashboard/icon.svg",
            get(|| async { asset(ICON, "image/svg+xml") }),
        )
        .layer(DefaultBodyLimit::max(MAX_FORM))
        .with_state(board)
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_secs())
}

/// When the request's login expires, in Unix seconds, if it has one.
fn login_expiry<S>(board: &Board<S>, headers: &HeaderMap) -> Option<u64> {
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .filter_map(auth::cookie)
        .find_map(|value| board.account.expiry(value, now_secs()))
}

fn signed_in<S>(board: &Board<S>, headers: &HeaderMap) -> bool {
    login_expiry(board, headers).is_some()
}

/// Headers every dashboard response carries.
fn hardened(mut response: Response) -> Response {
    let headers = response.headers_mut();
    let fixed = [
        (header::CACHE_CONTROL, "no-store"),
        (header::CONTENT_SECURITY_POLICY, CSP),
        (header::REFERRER_POLICY, "no-referrer"),
        (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
        (header::X_FRAME_OPTIONS, "DENY"),
    ];
    for (name, value) in fixed {
        headers.insert(name, HeaderValue::from_static(value));
    }
    response
}

fn html(body: String) -> Response {
    hardened(([(header::CONTENT_TYPE, "text/html; charset=utf-8")], body).into_response())
}

fn asset(body: &'static str, kind: &'static str) -> Response {
    hardened(([(header::CONTENT_TYPE, kind)], body).into_response())
}

fn redirect(to: &'static str, cookie: Option<String>) -> Response {
    let mut response = (StatusCode::SEE_OTHER, [(header::LOCATION, to)]).into_response();
    if let Some(cookie) = cookie.and_then(|cookie| HeaderValue::from_str(&cookie).ok()) {
        response.headers_mut().insert(header::SET_COOKIE, cookie);
    }
    hardened(response)
}

async fn index<S: Site>(State(board): State<Board<S>>, headers: HeaderMap) -> Response {
    if !signed_in(&board, &headers) {
        return redirect("/dashboard/login", None);
    }
    html(board.page.index.to_owned())
}

async fn login_page<S: Site>(State(board): State<Board<S>>, RawQuery(query): RawQuery) -> Response {
    let message = match query.as_deref() {
        Some("blocked") => BLOCKED,
        Some("failed") => FAILED,
        Some("limited") => LIMITED,
        _ => "",
    };
    html(
        LOGIN
            .replace(NAME_SLOT, board.page.name)
            .replace(MESSAGE_SLOT, message),
    )
}

async fn login<S: Site>(
    State(board): State<Board<S>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    // A page on another site cannot post here: otherwise it could submit
    // wrong tokens from a visitor's browser and get the visitor's address
    // locked out and banned. Refused before anything is counted.
    if !same_origin(&headers) {
        return hardened(StatusCode::FORBIDDEN.into_response());
    }
    let address = board.site.address(peer, &headers);
    let now = Instant::now();
    // A blocked address is refused before the limit on all addresses,
    // so its tries do not use that up and lock the operator out.
    if let Verdict::Blocked(_) = board.lockout.check(address, now) {
        board.site.login(Login::Blocked);
        return redirect("/dashboard/login?blocked", None);
    }
    let since_ms = u64::try_from(board.started.elapsed().as_millis()).unwrap_or(u64::MAX);
    if !allow(&board.everyone, (), since_ms) {
        board.site.login(Login::Limited);
        return redirect("/dashboard/login?limited", None);
    }
    // The try is reserved before the token is looked at, under one lock,
    // so a blocked address learns nothing and requests sent at once
    // cannot get more than three guesses between them.
    if let Verdict::Blocked(_) = board.lockout.attempt(address, now) {
        board.site.login(Login::Blocked);
        return redirect("/dashboard/login?blocked", None);
    }
    let accepted = auth::login_form(&body).is_some_and(|token| board.account.check(&token));
    if !accepted {
        board.site.login(Login::Failed);
        if board.lockout.exhausted(address, now) {
            // Blocked from the dashboard only, never banned from the
            // relay: everyone behind a shared address, a carrier's NAT
            // say, would lose their computers for one person's typos.
            tracing::warn!("an address was blocked after wrong dashboard tokens");
            return redirect("/dashboard/login?blocked", None);
        }
        tracing::warn!("a dashboard login failed");
        return redirect("/dashboard/login?failed", None);
    }
    board.lockout.succeed(address);
    board.site.login(Login::Succeeded);
    let Ok(cookie) = board.account.issue(now_secs()) else {
        return hardened(StatusCode::INTERNAL_SERVER_ERROR.into_response());
    };
    redirect("/dashboard", Some(auth::set_cookie(Some(&cookie))))
}

/// Signs this browser's session out on the relay, not only in the browser.
async fn logout<S: Site>(State(board): State<Board<S>>, headers: HeaderMap) -> Response {
    if !same_origin(&headers) {
        return hardened(StatusCode::FORBIDDEN.into_response());
    }
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .filter_map(auth::cookie)
        .for_each(|value| board.account.revoke(value));
    redirect("/dashboard/login", Some(auth::set_cookie(None)))
}

fn allow<K: Eq + std::hash::Hash>(limiter: &Mutex<Limiter<K>>, key: K, now_ms: u64) -> bool {
    match limiter.lock() {
        Ok(mut limiter) => limiter.allow(key, now_ms),
        Err(poisoned) => poisoned.into_inner().allow(key, now_ms),
    }
}

async fn stats<S: Site>(State(board): State<Board<S>>, headers: HeaderMap) -> Response {
    if !signed_in(&board, &headers) {
        return hardened(StatusCode::UNAUTHORIZED.into_response());
    }
    hardened(Json(board.site.stats()).into_response())
}

/// A snapshot a second until the login expires.
async fn live_stats<S: Site>(State(board): State<Board<S>>, headers: HeaderMap) -> Response {
    let Some(expires) = login_expiry(&board, &headers) else {
        return hardened(StatusCode::UNAUTHORIZED.into_response());
    };
    let Some(place) = board.live.open() else {
        return hardened(StatusCode::SERVICE_UNAVAILABLE.into_response());
    };
    let site = board.site;
    let (events, keep_alive) = live::events(place, expires, move || {
        serde_json::to_string(&site.stats()).unwrap_or_default()
    });
    hardened(Sse::new(events).keep_alive(keep_alive).into_response())
}

/// Whether a request comes from the relay's own pages, or from no page at
/// all. Browsers send `Sec-Fetch-Site`, or at least `Origin`, on every
/// cross-site form post and WebSocket; a client that sends neither, such
/// as `curl` or Sheprd, is not a browser being used against its owner.
#[must_use]
pub fn same_origin(headers: &HeaderMap) -> bool {
    let text = |name| {
        headers
            .get(name)
            .and_then(|value: &HeaderValue| value.to_str().ok())
    };
    if let Some(site) = text("sec-fetch-site") {
        return matches!(site, "same-origin" | "none");
    }
    let Some(origin) = text("origin") else {
        return true;
    };
    let host = text("host").unwrap_or_default();
    let origin_host = origin
        .strip_prefix("https://")
        .or_else(|| origin.strip_prefix("http://"));
    origin_host
        .is_some_and(|origin_host| !host.is_empty() && origin_host.eq_ignore_ascii_case(host))
}

/// Checks a scrape's `Authorization: Bearer` token with `matches`, counting
/// tries per `caller` in `lockout` as the login does: three wrong ones block
/// it for 15 minutes, unchecked, so a token cannot be guessed at the port's
/// speed. `Err` is the status to answer with.
pub fn bearer(
    lockout: &Lockout,
    caller: IpAddr,
    headers: &HeaderMap,
    matches: impl FnOnce(&[u8]) -> bool,
) -> Result<(), StatusCode> {
    let now = Instant::now();
    if let Verdict::Blocked(_) = lockout.attempt(caller, now) {
        return Err(StatusCode::TOO_MANY_REQUESTS);
    }
    let given = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .unwrap_or_default();
    // `attempt` counted this try as wrong; a right one clears it.
    if !matches(given.as_bytes()) {
        return Err(StatusCode::UNAUTHORIZED);
    }
    lockout.succeed(caller);
    Ok(())
}
