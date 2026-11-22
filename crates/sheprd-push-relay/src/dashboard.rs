//! What the operator sees: the read-only dashboard and `GET /metrics`,
//! both served only when the secrets hold a `dashboard-token`, and both
//! behind it. The dashboard's routes, login, and lockout are
//! `sheprd-dashboard`'s, shared with the relay; this is the push relay's
//! page and the numbers it draws, which are counts alone ([`crate::metrics`]).
//!
//! `/metrics` takes the same token as `Authorization: Bearer TOKEN`, and
//! three wrong ones block the caller for 15 minutes, as the login does.

use crate::{
    metrics::{Login, Metrics, Totals},
    web::{self, Source},
};
use axum::{
    Router,
    extract::{ConnectInfo, State},
    http::{HeaderMap, header},
    response::{IntoResponse, Response},
    routing::get,
};
use serde::Serialize;
use sheprd_apns::Host;
use sheprd_dashboard::{
    Page, Site, auth::Account, bearer, lockout::Lockout, metrics::CONTENT_TYPE,
};
use std::{
    net::{IpAddr, SocketAddr},
    sync::Arc,
};

const PAGE: Page = Page {
    index: include_str!("dashboard/index.html"),
    name: "Sheprd push relay",
    script: include_str!("dashboard/app.js"),
};

/// The push relay as its dashboard and scrapes read it.
#[derive(Clone, Debug)]
struct Board {
    environment: Host,
    metrics: Arc<Metrics>,
    source: Source,
}

/// The numbers the page draws.
#[derive(Debug, Serialize)]
pub struct Stats {
    /// The APNs environment this relay serves: `production` or `sandbox`.
    pub environment: &'static str,
    pub totals: Totals,
    pub version: &'static str,
}

impl Site for Board {
    type Stats = Stats;

    fn address(&self, peer: SocketAddr, headers: &HeaderMap) -> IpAddr {
        web::source(self.source, peer, headers)
    }

    fn login(&self, outcome: Login) {
        self.metrics.login(outcome);
    }

    fn stats(&self) -> Stats {
        Stats {
            environment: match self.environment {
                Host::Production => "production",
                Host::Sandbox => "sandbox",
            },
            totals: self.metrics.totals(web::now_ms() / 1000),
            version: env!("CARGO_PKG_VERSION"),
        }
    }
}

/// What a scrape needs.
#[derive(Clone, Debug)]
struct Scrape {
    account: Arc<Account>,
    board: Board,
    /// Wrong tokens per caller, apart from the login's.
    lockout: Arc<Lockout>,
}

/// The dashboard's routes and `/metrics`, for `account`'s token.
pub fn router(
    metrics: Arc<Metrics>,
    environment: Host,
    source: Source,
    account: Account,
) -> Router {
    let account = Arc::new(account);
    let board = Board {
        environment,
        metrics,
        source,
    };
    let scrape = Router::new()
        .route("/metrics", get(scrape))
        .with_state(Scrape {
            account: Arc::clone(&account),
            board: board.clone(),
            lockout: Arc::default(),
        });
    sheprd_dashboard::router(board, account, PAGE).merge(scrape)
}

async fn scrape(
    State(scrape): State<Scrape>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
) -> Response {
    let caller = scrape.board.address(peer, &headers);
    let checked = bearer(&scrape.lockout, caller, &headers, |given| {
        std::str::from_utf8(given).is_ok_and(|given| scrape.account.check(given))
    });
    if let Err(status) = checked {
        return status.into_response();
    }
    let text = scrape.board.metrics.render(web::now_ms() / 1000);
    (
        [
            (header::CONTENT_TYPE, CONTENT_TYPE),
            (header::CACHE_CONTROL, "no-store"),
        ],
        text,
    )
        .into_response()
}
