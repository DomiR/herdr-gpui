//! The dashboard and `/metrics` over HTTP: absent without a token, and
//! behind it with one.

use super::*;
use crate::web::{Source, router};
use axum::{
    Router,
    body::Body,
    extract::ConnectInfo,
    http::{Request, StatusCode, header},
};
use http_body_util::BodyExt as _;
use sheprd_dashboard::auth::Account;
use std::net::SocketAddr;
use tower::ServiceExt as _;

const TOKEN: &str = "0123456789abcdef0123456789abcdef";

fn app(token: Option<&str>) -> Router {
    let Fixture { relay, .. } = fixture();
    let account = token.map(|token| Account::new(token).unwrap());
    router(relay, Source::Proxy, account)
}

/// A request from `from`, as Caddy forwards it.
fn get(path: &str, from: &str, headers: &[(header::HeaderName, &str)]) -> Request<Body> {
    let mut request = Request::get(path).header("x-forwarded-for", from);
    for (name, value) in headers {
        request = request.header(name, *value);
    }
    let mut request = request.body(Body::empty()).unwrap();
    let peer: SocketAddr = "10.0.0.2:4000".parse().unwrap();
    request.extensions_mut().insert(ConnectInfo(peer));
    request
}

async fn send(app: &Router, request: Request<Body>) -> (StatusCode, String) {
    let response = app.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (status, String::from_utf8(bytes.to_vec()).unwrap())
}

const PATHS: [&str; 5] = [
    "/metrics",
    "/dashboard",
    "/dashboard/login",
    "/dashboard/api/stats",
    "/dashboard/api/live",
];

#[tokio::test]
async fn without_a_token_there_is_no_dashboard_or_metrics() {
    let app = app(None);
    for path in PATHS {
        let (status, _) = send(&app, get(path, "203.0.113.9", &[])).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{path}");
    }
    let (status, _) = send(&app, get("/healthz", "203.0.113.9", &[])).await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn with_a_token_nothing_is_shown_without_it() {
    let app = app(Some(TOKEN));
    let (status, _) = send(&app, get("/metrics", "203.0.113.9", &[])).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let (status, _) = send(&app, get("/dashboard", "203.0.113.9", &[])).await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    for path in ["/dashboard/api/stats", "/dashboard/api/live"] {
        let (status, _) = send(&app, get(path, "203.0.113.9", &[])).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "{path}");
    }
    let (status, page) = send(&app, get("/dashboard/login", "203.0.113.9", &[])).await;
    assert_eq!(status, StatusCode::OK);
    assert!(page.contains("Sheprd push relay"));
}

#[tokio::test]
async fn the_token_opens_metrics_and_three_wrong_ones_block_the_caller() {
    let app = app(Some(TOKEN));
    let right = format!("Bearer {TOKEN}");
    let (status, text) = send(
        &app,
        get(
            "/metrics",
            "203.0.113.9",
            &[(header::AUTHORIZATION, &right)],
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(text.contains("sheprd_push_relay_pushes_total{outcome=\"delivered\"} 0"));

    let wrong = [(header::AUTHORIZATION, "Bearer not the token")];
    for _ in 0..3 {
        let (status, _) = send(&app, get("/metrics", "198.51.100.4", &wrong)).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }
    let authorized = [(header::AUTHORIZATION, right.as_str())];
    let (status, _) = send(&app, get("/metrics", "198.51.100.4", &authorized)).await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
    // Keyed by the address Caddy reports, not the proxy's own.
    let (status, _) = send(&app, get("/metrics", "203.0.113.9", &authorized)).await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn a_login_opens_the_stats() {
    let app = app(Some(TOKEN));
    let mut login = Request::post("/dashboard/login")
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .header("x-forwarded-for", "203.0.113.9")
        .body(Body::from(format!("token={TOKEN}")))
        .unwrap();
    let peer: SocketAddr = "10.0.0.2:4000".parse().unwrap();
    login.extensions_mut().insert(ConnectInfo(peer));
    let response = app.clone().oneshot(login).await.unwrap();
    assert_eq!(response.headers()[header::LOCATION], "/dashboard");
    let cookie = response.headers()[header::SET_COOKIE].to_str().unwrap();
    let pair = cookie.split(';').next().unwrap();
    let (status, json) = send(
        &app,
        get(
            "/dashboard/api/stats",
            "203.0.113.9",
            &[(header::COOKIE, pair)],
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let json: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(json["environment"], "production");
    assert_eq!(json["totals"]["devices"]["active_day"], 0);
    assert_eq!(json["totals"]["logins_failed"], 0);
}

#[cfg(unix)]
#[test]
fn the_token_file_is_optional_but_must_be_private_and_long() {
    // Only this Unix test reads the token file: imported here, Windows
    // builds of the tests have no unused import.
    use crate::serve::read_dashboard_token;
    use std::os::unix::fs::PermissionsExt as _;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("dashboard-token");
    assert!(read_dashboard_token(&path).unwrap().is_none());

    std::fs::write(&path, format!("{TOKEN}\n")).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
    let account = read_dashboard_token(&path).unwrap().unwrap();
    assert!(account.check(TOKEN));

    std::fs::write(&path, "short").unwrap();
    assert!(matches!(
        read_dashboard_token(&path),
        Err(crate::Error::Dashboard(
            sheprd_dashboard::Error::WeakSecret { .. }
        ))
    ));
    std::fs::write(&path, TOKEN).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
    assert!(matches!(
        read_dashboard_token(&path),
        Err(crate::Error::NotPrivate { .. })
    ));
}
