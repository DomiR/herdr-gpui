//! `/api/v2/favorites`: a phone stars or unstars one agent, by pane, and
//! every live home hears of it.

use super::*;
use crate::control::favorites::Favorites;
use serde_json::{Value, json};

fn post(token: Option<&str>, body: &Value) -> HttpRequest<Body> {
    let mut request = HttpRequest::post("/api/v2/favorites")
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::ORIGIN, ORIGIN)
        .body(Body::from(body.to_string()))
        .unwrap();
    if let Some(token) = token {
        request.headers_mut().insert(
            header::AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {token}")).unwrap(),
        );
    }
    request
}

#[tokio::test]
async fn favorites_need_a_session() {
    let dir = tempfile::tempdir().unwrap();
    let state = state(dir.path());
    let change = json!({"pane": "p1", "favorite": true});
    assert_eq!(
        send(&state, post(None, &change)).await.0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(state.favorites.current(), Favorites::default());
}

#[tokio::test]
async fn a_phone_stars_and_unstars_an_agent() {
    let dir = tempfile::tempdir().unwrap();
    let state = state(dir.path());
    let (_, token) = signed_in(&state, 1);
    let announced = state.favorites.subscribe();

    let starred = json!({"pane": "p1", "favorite": true});
    let (status, _, body) = send(&state, post(Some(&token), &starred)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        serde_json::from_str::<Value>(&body).unwrap(),
        json!({"panes": ["p1"]})
    );
    assert!(announced.has_changed().unwrap(), "live homes hear of it");
    assert_eq!(
        crate::app::favorites::load(&crate::app::favorites::file(dir.path())).panes,
        ["p1"],
        "saved before anyone hears of it"
    );

    let unstarred = json!({"pane": "p1", "favorite": false});
    let (status, _, body) = send(&state, post(Some(&token), &unstarred)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        serde_json::from_str::<Value>(&body).unwrap(),
        json!({"panes": []})
    );
}

#[tokio::test]
async fn a_pane_that_is_no_id_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let state = state(dir.path());
    let (_, token) = signed_in(&state, 1);
    let change = json!({"pane": " ", "favorite": true});
    assert_eq!(
        send(&state, post(Some(&token), &change)).await.0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(state.favorites.current(), Favorites::default());
}

#[test]
fn a_new_primary_boot_forgets_its_panes() {
    let dir = tempfile::tempdir().unwrap();
    let state = state(dir.path());
    for pane in ["p1", "herdr:work:p1"] {
        state
            .favorites
            .change(crate::control::favorites::Change {
                pane: pane.into(),
                favorite: true,
            })
            .unwrap();
    }
    state.favorites.follow_boot("boot-1").unwrap();
    assert_eq!(state.favorites.current().panes.len(), 2);
    let after = state.favorites.follow_boot("boot-2").unwrap();
    assert_eq!(after.panes, ["herdr:work:p1"]);
    assert_eq!(
        crate::app::favorites::load(&crate::app::favorites::file(dir.path())),
        after
    );
}
