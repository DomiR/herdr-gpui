//! The routes over a fake site: the login, the page and stats behind it,
//! and the bearer check for scrapes.

use super::*;
use crate::{bearer, lockout::Lockout, same_origin};
use axum::http::StatusCode;

#[tokio::test]
async fn the_login_names_the_site_and_the_stats_need_it() {
    let (router, site) = app();
    let page = send(
        &router,
        Request::get("/dashboard/login")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    let page = text(page).await;
    assert!(
        page.contains("<title>Fake relay · Sign in</title>"),
        "{page}"
    );
    assert!(!page.contains("__NAME__"));

    let stats = Request::get("/dashboard/api/stats")
        .body(Body::empty())
        .unwrap();
    assert_eq!(
        send(&router, stats).await.status(),
        StatusCode::UNAUTHORIZED
    );
    let wrong = send(&router, login("a wrong token of enough length")).await;
    assert_eq!(wrong.headers()[header::LOCATION], "/dashboard/login?failed");

    let right = send(&router, login(TOKEN)).await;
    assert_eq!(right.headers()[header::LOCATION], "/dashboard");
    let cookie = right.headers()[header::SET_COOKIE].to_str().unwrap();
    let pair = cookie.split(';').next().unwrap().to_owned();
    let stats = Request::get("/dashboard/api/stats")
        .header(header::COOKIE, &pair)
        .body(Body::empty())
        .unwrap();
    assert_eq!(text(send(&router, stats).await).await, r#"{"answer":42}"#);
    let index = Request::get("/dashboard")
        .header(header::COOKIE, &pair)
        .body(Body::empty())
        .unwrap();
    assert_eq!(text(send(&router, index).await).await, PAGE.index);
    assert_eq!(
        *site.logins.lock().unwrap(),
        [Login::Failed, Login::Succeeded]
    );
}

#[tokio::test]
async fn the_sites_script_is_served() {
    let (router, _) = app();
    let script = send(
        &router,
        Request::get("/dashboard/app.js")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(text(script).await, PAGE.script);
}

fn authorization(value: &str) -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(header::AUTHORIZATION, value.parse().unwrap());
    headers
}

#[test]
fn a_scrape_needs_the_token_and_three_wrong_ones_block_it() {
    let lockout = Lockout::default();
    let caller: IpAddr = "203.0.113.9".parse().unwrap();
    let check = |given: &[u8]| given == TOKEN.as_bytes();
    let right = authorization(&format!("Bearer {TOKEN}"));
    assert_eq!(bearer(&lockout, caller, &right, check), Ok(()));
    assert_eq!(
        bearer(&lockout, caller, &HeaderMap::new(), check),
        Err(StatusCode::UNAUTHORIZED)
    );
    let wrong = authorization("Bearer not it");
    for _ in 0..2 {
        assert_eq!(
            bearer(&lockout, caller, &wrong, check),
            Err(StatusCode::UNAUTHORIZED)
        );
    }
    assert_eq!(
        bearer(&lockout, caller, &right, check),
        Err(StatusCode::TOO_MANY_REQUESTS)
    );
}

#[test]
fn a_cross_site_form_is_not_same_origin() {
    let mut headers = HeaderMap::new();
    assert!(same_origin(&headers));
    headers.insert("sec-fetch-site", "cross-site".parse().unwrap());
    assert!(!same_origin(&headers));
}
