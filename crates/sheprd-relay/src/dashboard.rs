//! The read-only dashboard, behind one access token: connections,
//! sessions, traffic, and refusals, live. It is served only when the
//! operator set a token, and changes nothing on the relay.
//!
//! The routes, the login and its lockout, and the live stream are
//! `sheprd-dashboard`'s, shared with the push relay; this is the relay's
//! page and the numbers it draws.

pub mod history;

pub use sheprd_dashboard::{auth, live, lockout};

use crate::{
    guard::BanRow,
    hub::{Hub, SessionRow},
    metrics::{Login, Totals},
    web::Shared,
};
use auth::Account;
use axum::{Router, http::HeaderMap};
use history::{History, Sample};
use serde::Serialize;
use sheprd_dashboard::{Page, Site};
use std::{
    net::{IpAddr, SocketAddr},
    sync::{Arc, Mutex},
    time::Instant,
};

const PAGE: Page = Page {
    index: include_str!("dashboard/assets/index.html"),
    name: "Sheprd relay",
    script: include_str!("dashboard/assets/app.js"),
};

/// The relay as its dashboard reads it.
#[derive(Clone, Debug)]
struct Relay {
    history: Arc<Mutex<History>>,
    shared: Shared,
}

impl Site for Relay {
    type Stats = Stats;

    fn address(&self, peer: SocketAddr, headers: &HeaderMap) -> IpAddr {
        self.shared.address(peer, headers)
    }

    fn login(&self, outcome: Login) {
        self.shared.hub.metrics.login(outcome);
    }

    fn stats(&self) -> Stats {
        let history = match self.history.lock() {
            Ok(history) => history.samples(),
            Err(poisoned) => poisoned.into_inner().samples(),
        };
        snapshot(&self.shared.hub, history)
    }
}

/// The dashboard's routes, for an operator `account`.
///
/// Starts sampling throughput, so it must run inside a Tokio runtime.
pub fn router(shared: Shared, account: Account) -> Router {
    let history = Arc::new(Mutex::new(History::default()));
    let hub = Arc::clone(&shared.hub);
    tokio::spawn(history::run(Arc::downgrade(&history), move || {
        let totals = hub.metrics.totals();
        (totals.bytes_from_dialers, totals.bytes_from_listeners)
    }));
    sheprd_dashboard::router(Relay { history, shared }, Arc::new(account), PAGE)
}

/// The numbers the page draws.
#[derive(Debug, Serialize)]
pub struct Stats {
    /// Bans now, earned and manual.
    pub bans: Vec<BanRow>,
    /// Throughput samples, oldest first.
    pub history: Vec<Sample>,
    pub limits: LimitsView,
    pub sessions: Vec<SessionRow>,
    pub totals: Totals,
    pub version: &'static str,
}

/// The limits the operator set, as the page shows them.
#[derive(Debug, Serialize)]
pub struct LimitsView {
    pub max_connections: usize,
    pub max_connections_per_ip: usize,
    pub max_listeners: usize,
    pub max_sessions: usize,
    pub session_bandwidth: u64,
    pub total_bandwidth: u64,
}

/// The stats for `hub` as the dashboard reads them.
#[must_use]
pub fn snapshot(hub: &Hub, history: Vec<Sample>) -> Stats {
    let limits = hub.limits;
    Stats {
        bans: hub.guard.bans(Instant::now()),
        history,
        limits: LimitsView {
            max_connections: limits.max_connections,
            max_connections_per_ip: limits.max_connections_per_ip,
            max_listeners: limits.max_listeners,
            max_sessions: limits.max_sessions,
            session_bandwidth: limits.session_bandwidth,
            total_bandwidth: limits.total_bandwidth,
        },
        sessions: hub.sessions(),
        totals: hub.metrics.totals(),
        version: env!("CARGO_PKG_VERSION"),
    }
}
