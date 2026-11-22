//! What goes through the push relay is sealed: neither the relay nor APNs
//! reads a chat's name, step, question, or computer, and it still fits.

use super::*;
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use p256::SecretKey;
use serde_json::Value;

pub(super) fn phone() -> SecretKey {
    SecretKey::from_slice(&[5; 32]).unwrap()
}

pub(super) fn start(text: &str) -> Live {
    Live {
        event: Event::Start {
            alert: Alert {
                body: format!("{text} workspace"),
                title: "Claude is working".to_owned(),
            },
            attributes: Attributes {
                agent: "Claude".to_owned(),
                chat_title: format!("{text} title"),
                computer: "studio".to_owned(),
                computer_id: "AB".repeat(32),
                pane: "w1-p3".to_owned(),
                terminal: false,
                workspace: format!("{text} workspace"),
            },
        },
        pane: "w1-p3".to_owned(),
        state: ContentState {
            options: Vec::new(),
            permission: Some(7),
            phase: Phase::Question,
            plan_done: Some(1),
            plan_total: Some(2),
            previous: vec![format!("{text} earlier")],
            question: Some(format!("{text} question")),
            started_at: 1_799_999_990,
            step: format!("{text} step"),
            summary: None,
            waiting: 1,
            working: 1,
        },
    }
}

pub(super) fn payload(live: &Live) -> Value {
    let push = live
        .sealed_push(&phone().public_key(), 1_800_000_000)
        .unwrap();
    assert_eq!(push.push().push_type, PushType::LiveActivity);
    assert!(push.push().body.len() <= sheprd_apns::MAX_PAYLOAD);
    serde_json::from_slice(&push.push().body).unwrap()
}

pub(super) fn open(field: &Value) -> Value {
    let sealed = URL_SAFE_NO_PAD
        .decode(field["sealed"].as_str().unwrap())
        .unwrap();
    serde_json::from_slice(&seal::open(&phone(), &sealed, seal::ACTIVITY_INFO).unwrap()).unwrap()
}

#[test]
fn a_relayed_start_carries_only_what_ios_reads() {
    let live = start("secret");
    let payload = payload(&live);
    let text = payload.to_string();
    for hidden in ["secret", "Claude", "studio", &"AB".repeat(32), "w1-p3"] {
        assert!(!text.contains(hidden), "{hidden} in {text}");
    }
    let aps = &payload["aps"];
    assert_eq!(aps["event"], "start");
    assert_eq!(aps["timestamp"], 1_800_000_000);
    assert_eq!(aps["attributes-type"], ATTRIBUTES_TYPE);
    assert_eq!(aps["alert"]["title"], "Sheprd");
    assert_eq!(aps["alert"]["body"], "New activity");
    assert_eq!(aps["content-state"].as_object().unwrap().len(), 1);

    // The phone opens exactly what own-key mode would have sent in plain.
    let Event::Start { attributes, .. } = &live.event else {
        unreachable!()
    };
    assert_eq!(
        open(&aps["attributes"]),
        serde_json::to_value(attributes).unwrap()
    );
    assert_eq!(
        open(&aps["content-state"]),
        serde_json::to_value(&live.state).unwrap()
    );
}

#[test]
fn a_relayed_update_and_end_keep_their_dates_and_the_cover() {
    let mut update = start("secret");
    update.event = Event::Update {
        alert: Some(Alert {
            body: "secret".to_owned(),
            title: "secret".to_owned(),
        }),
    };
    let aps = payload(&update)["aps"].clone();
    assert_eq!(aps["alert"]["body"], "New activity");
    assert!(aps.get("attributes").is_none());
    assert!(!aps.to_string().contains("secret"));

    let mut end = start("secret");
    end.event = Event::End {
        dismissal_s: 1_800_001_262,
    };
    let aps = payload(&end)["aps"].clone();
    assert_eq!(aps["dismissal-date"], 1_800_001_262);
    assert!(aps.get("alert").is_none());
}

#[test]
fn relayed_content_is_not_an_alert() {
    let aps = payload(&start("x"))["aps"].clone();
    let sealed = URL_SAFE_NO_PAD
        .decode(aps["content-state"]["sealed"].as_str().unwrap())
        .unwrap();
    assert!(seal::open(&phone(), &sealed, seal::INFO).is_err());
}

#[test]
fn text_too_long_to_seal_is_cut_until_it_fits() {
    let emoji = "\u{1f600}".repeat(400);
    let mut live = start(&emoji);
    live.state.previous = vec![emoji.clone(), emoji.clone()];
    assert!(matches!(
        live.payload(0),
        Err(Error::PayloadTooLarge { .. })
    ));
    let opened = open(&payload(&live)["aps"]["content-state"]);
    assert!(opened.get("previous").is_none());
    let step = opened["step"].as_str().unwrap();
    assert!(step.ends_with('\u{2026}'), "{step}");
    assert!(step.len() < live.state.step.len());
}

#[test]
fn buttons_too_long_to_seal_go_with_their_permission() {
    use sheprd_chat_protocol::{PermissionKind, PermissionOption};
    let mut live = start("q");
    live.event = Event::Update { alert: None };
    live.state.options = (0..3)
        .map(|n| PermissionOption {
            id: format!("option-{n}"),
            kind: PermissionKind::AllowOnce,
            name: "\u{1f600}".repeat(250),
        })
        .collect();
    let opened = open(&payload(&live)["aps"]["content-state"]);
    assert!(opened.get("options").is_none(), "{opened}");
    assert!(opened.get("permission").is_none(), "{opened}");
    // The question stays, for the phone to answer in the app.
    assert!(opened["question"].as_str().unwrap().starts_with('q'));
}

#[test]
fn buttons_that_fit_are_kept() {
    use sheprd_chat_protocol::{PermissionKind, PermissionOption};
    let mut live = start("q");
    live.state.options = vec![PermissionOption {
        id: "allow".to_owned(),
        kind: PermissionKind::AllowOnce,
        name: "Allow once".to_owned(),
    }];
    let opened = open(&payload(&live)["aps"]["content-state"]);
    assert_eq!(opened["options"][0]["name"], "Allow once");
    assert_eq!(opened["permission"], 7);
}
