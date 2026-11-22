//! The HTTP API. Bodies are bounded and parsed here, so that a parse error's
//! text, which can quote the request, never reaches a response or a log.
//! With a dashboard token, the dashboard and `/metrics` are merged in from
//! [`crate::dashboard`].

use crate::{
    dashboard,
    relay::{AttestRequest, Gateway, Issued, PushRequest, Relay, Reply, RevokeRequest},
    store::Store,
};
use axum::{
    Json, Router,
    body::Bytes,
    extract::{ConnectInfo, DefaultBodyLimit, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use serde::{Serialize, de::DeserializeOwned};
use sheprd_dashboard::auth::Account;
use std::{
    net::{IpAddr, SocketAddr},
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

/// A push's handle and payload, with room for JSON escaping.
const MAX_BODY: usize = 16 * 1024;

/// How to tell who a request came from.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Source {
    /// The connection's peer.
    Peer,
    /// The last `X-Forwarded-For` entry, which the proxy in front appended.
    /// Only for a relay no one can reach except through that proxy.
    Proxy,
}

struct Shared<G, S> {
    relay: Relay<G, S>,
    source: Source,
}

#[derive(Serialize)]
struct Body {
    outcome: Reply,
}

#[derive(Serialize)]
struct Challenge {
    challenge: String,
}

impl IntoResponse for Reply {
    fn into_response(self) -> Response {
        let status = match self {
            Self::Delivered | Self::Revoked => StatusCode::OK,
            Self::Expired => StatusCode::UNAUTHORIZED,
            Self::Gone => StatusCode::GONE,
            Self::Invalid => StatusCode::BAD_REQUEST,
            Self::Limited => StatusCode::TOO_MANY_REQUESTS,
            Self::Refused => StatusCode::FORBIDDEN,
            Self::Unavailable => StatusCode::SERVICE_UNAVAILABLE,
        };
        (status, Json(Body { outcome: self })).into_response()
    }
}

/// The relay's routes, and with `dashboard`, the dashboard and `/metrics`
/// behind its token. Without it, neither exists.
pub fn router<G: Gateway, S: Store>(
    relay: Relay<G, S>,
    source: Source,
    dashboard: Option<Account>,
) -> Router {
    let board = dashboard.map(|account| {
        let metrics = Arc::clone(relay.metrics());
        dashboard::router(metrics, relay.environment(), source, account)
    });
    let api = Router::new()
        .route("/healthz", get(|| async { "ok" }))
        .route("/v1/attest", post(attest::<G, S>))
        .route("/v1/challenge", post(challenge::<G, S>))
        .route("/v1/push", post(push::<G, S>))
        .route("/v1/revoke", post(revoke::<G, S>))
        .layer(DefaultBodyLimit::max(MAX_BODY))
        .with_state(Arc::new(Shared { relay, source }));
    match board {
        Some(board) => api.merge(board),
        None => api,
    }
}

async fn push<G: Gateway, S: Store>(
    State(shared): State<Arc<Shared<G, S>>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    body: Bytes,
) -> Reply {
    let source = source(shared.source, peer, &headers);
    let now_ms = now_ms();
    match parsed::<PushRequest, G, S>(&shared.relay, source, &body, now_ms) {
        Ok(request) => shared.relay.push(source, request, now_ms).await,
        Err(reply) => {
            shared.relay.metrics().push(reply);
            reply
        }
    }
}

async fn challenge<G: Gateway, S: Store>(
    State(shared): State<Arc<Shared<G, S>>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
) -> Result<Json<Challenge>, Reply> {
    let source = source(shared.source, peer, &headers);
    let challenge = shared.relay.challenge(source, now_ms())?;
    Ok(Json(Challenge { challenge }))
}

async fn attest<G: Gateway, S: Store>(
    State(shared): State<Arc<Shared<G, S>>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<Issued>, Reply> {
    let source = source(shared.source, peer, &headers);
    let now_ms = now_ms();
    let request = parsed::<AttestRequest, G, S>(&shared.relay, source, &body, now_ms)
        .inspect_err(|reply| shared.relay.metrics().attestation((*reply).into()))?;
    shared.relay.attest(source, &request, now_ms).map(Json)
}

async fn revoke<G: Gateway, S: Store>(
    State(shared): State<Arc<Shared<G, S>>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    body: Bytes,
) -> Reply {
    let source = source(shared.source, peer, &headers);
    let now_ms = now_ms();
    match parsed::<RevokeRequest, G, S>(&shared.relay, source, &body, now_ms) {
        Ok(request) => shared.relay.revoke(source, request, now_ms).await,
        Err(reply) => {
            shared.relay.metrics().revocation(reply);
            reply
        }
    }
}

/// The request in `body`. A malformed one still spends the source's limit,
/// or junk could be sent without any.
fn parsed<T: DeserializeOwned, G: Gateway, S: Store>(
    relay: &Relay<G, S>,
    source: IpAddr,
    body: &[u8],
    now_ms: u64,
) -> Result<T, Reply> {
    serde_json::from_slice(body).map_err(|_| {
        if relay.source_allowed(source, now_ms) {
            Reply::Invalid
        } else {
            Reply::Limited
        }
    })
}

/// The address limits key a request by.
pub(crate) fn source(source: Source, peer: SocketAddr, headers: &HeaderMap) -> IpAddr {
    let forwarded = || {
        headers
            .get_all("x-forwarded-for")
            .iter()
            .next_back()?
            .to_str()
            .ok()?
            .rsplit(',')
            .next()?
            .trim()
            .parse()
            .ok()
    };
    match source {
        Source::Peer => peer.ip(),
        Source::Proxy => forwarded().unwrap_or_else(|| peer.ip()),
    }
}

/// The time now, in milliseconds since the Unix epoch.
pub(crate) fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| {
            u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX)
        })
}
