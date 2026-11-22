#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::{Page, Site, auth::Account, metrics::Login};
use axum::{
    Router,
    body::Body,
    extract::connect_info::MockConnectInfo,
    http::{HeaderMap, Request, Response, header},
};
use http_body_util::BodyExt as _;
use serde::Serialize;
use std::{
    net::{IpAddr, SocketAddr},
    sync::{Arc, Mutex},
};
use tower::ServiceExt as _;

mod board;
mod metrics;

const TOKEN: &str = "a dashboard token of enough length";

/// A site that counts logins and draws one number.
#[derive(Clone, Debug, Default)]
struct Fake {
    logins: Arc<Mutex<Vec<Login>>>,
}

#[derive(Serialize)]
struct Stats {
    answer: u64,
}

impl Site for Fake {
    type Stats = Stats;

    fn address(&self, peer: SocketAddr, _headers: &HeaderMap) -> IpAddr {
        peer.ip()
    }

    fn login(&self, outcome: Login) {
        self.logins.lock().unwrap().push(outcome);
    }

    fn stats(&self) -> Stats {
        Stats { answer: 42 }
    }
}

const PAGE: Page = Page {
    index: "<!doctype html><title>Fake</title>",
    name: "Fake relay",
    script: "\"use strict\";",
};

fn app() -> (Router, Fake) {
    let site = Fake::default();
    let router = crate::router(site.clone(), Arc::new(Account::new(TOKEN).unwrap()), PAGE)
        .layer(MockConnectInfo(SocketAddr::from(([198, 51, 100, 7], 1))));
    (router, site)
}

async fn send(router: &Router, request: Request<Body>) -> Response<Body> {
    router.clone().oneshot(request).await.unwrap()
}

async fn text(response: Response<Body>) -> String {
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    String::from_utf8(bytes.to_vec()).unwrap()
}

fn login(token: &str) -> Request<Body> {
    Request::post("/dashboard/login")
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(Body::from(format!("token={}", token.replace(' ', "+"))))
        .unwrap()
}
