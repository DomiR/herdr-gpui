//! The daemon log's view of the web app: one line per request, and the
//! errors phones report about themselves, which otherwise stay on a screen
//! nobody can inspect.

use super::{ApiError, Peer, Shared, now_ms};
use crate::auth::RateLimiter;
use axum::{
    Json,
    extract::{ConnectInfo, MatchedPath, Request, State},
    http::{HeaderMap, Method, StatusCode},
    middleware::Next,
    response::Response,
};
use serde::Deserialize;
use std::{net::IpAddr, sync::PoisonError, time::Instant};

/// The longest report kept, in characters; the rest is cut.
const MAX_REPORT_CHARS: usize = 600;
/// Reports a phone may send at once, earning one back each refill.
const REPORT_BURST: u32 = 10;
const REPORT_REFILL_MS: u64 = 6_000;
const REPORT_ADDRESSES: usize = 64;

/// Bounds how often each address may write to the log.
pub(crate) fn report_limit() -> RateLimiter<IpAddr> {
    RateLimiter::new(REPORT_BURST, REPORT_REFILL_MS, REPORT_ADDRESSES)
}

/// The header a phone names each call by (a few random hex digits), so a
/// failure it reports can be found here, or found missing.
const CALL_HEADER: &str = "x-sheprd-request";
/// The longest call tag kept; a longer or odd one is left out.
const MAX_CALL_CHARS: usize = 16;

/// Logs each request by its route, never its path or query: those carry
/// claim IDs and handoff tokens. A request whose client stopped waiting
/// before the answer is logged too, as one that ended unanswered: the
/// handler is dropped then, and would otherwise leave no line at all.
pub(super) async fn requests(request: Request, next: Next) -> Response {
    let mut line = Line {
        method: request.method().clone(),
        route: request
            .extensions()
            .get::<MatchedPath>()
            .map_or("(no route)", MatchedPath::as_str)
            .to_owned(),
        peer: request
            .extensions()
            .get::<ConnectInfo<Peer>>()
            .map(|ConnectInfo(Peer(peer))| peer.ip()),
        call: call_tag(request.headers()),
        started: Instant::now(),
        status: None,
    };
    let response = next.run(request).await;
    line.status = Some(response.status().as_u16());
    response
}

/// One request's log line, written when it is answered or dropped.
struct Line {
    method: Method,
    route: String,
    peer: Option<IpAddr>,
    call: Option<String>,
    started: Instant,
    status: Option<u16>,
}

impl Drop for Line {
    fn drop(&mut self) {
        let Self {
            method,
            route,
            peer,
            call,
            started,
            status,
        } = &*self;
        let peer = peer.map(tracing::field::display);
        let call = call.as_deref();
        let ms = started.elapsed().as_millis();
        match *status {
            Some(status) => tracing::info!(peer, %method, %route, call, status, ms, "request"),
            None => tracing::warn!(peer, %method, %route, call, ms, "request ended unanswered"),
        }
    }
}

/// The phone's tag for this call, when it is a short run of letters and
/// digits; anything else is not written to the log.
fn call_tag(headers: &HeaderMap) -> Option<String> {
    let tag = headers.get(CALL_HEADER)?.to_str().ok()?;
    let plain = !tag.is_empty()
        && tag.len() <= MAX_CALL_CHARS
        && tag.bytes().all(|byte| byte.is_ascii_alphanumeric());
    plain.then(|| tag.to_owned())
}

/// `POST /api/log` `{ context, message }`: an error the phone hit, such as
/// a failed pairing. Unauthenticated, as pairing fails before there is a
/// session, so it is bounded and rate limited per address.
#[derive(Deserialize)]
pub(super) struct Report {
    context: String,
    message: String,
}

pub(super) async fn report(
    State(state): State<Shared>,
    ConnectInfo(Peer(peer)): ConnectInfo<Peer>,
    Json(report): Json<Report>,
) -> Result<StatusCode, ApiError> {
    let allowed = state
        .reports
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .allow(peer.ip(), now_ms());
    if !allowed {
        return Err(ApiError::Auth(crate::auth::Error::RateLimited));
    }
    // Debug formatting escapes control characters, so a report cannot
    // forge log lines.
    tracing::warn!(
        peer = %peer.ip(),
        context = ?clip(&report.context),
        message = ?clip(&report.message),
        "phone reported an error"
    );
    Ok(StatusCode::NO_CONTENT)
}

fn clip(text: &str) -> &str {
    text.char_indices()
        .nth(MAX_REPORT_CHARS)
        .map_or(text, |(end, _)| &text[..end])
}

#[cfg(test)]
mod tests;
