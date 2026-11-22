//! A phone that keeps its own copy of the log, as the iOS app does
//! (`crates/sheprd-fleet-ffi`), against the daemon's `/api/v2/fleet/sync`:
//! the copy starts from the fleet's founding operation, the phone drafts
//! on its own heads, and what it signs applies here as it applied there.

use super::fleet::{Phone, admit, ask, call, device_key, fleet_id, founded};
use super::*;
use crate::control::fleet::{
    Body, Link, Log, Op, OpId, Reason, Remove,
    sync::{self, Answer, Ask},
};
use serde_json::{Value, json};

/// The phone's copy, started from what the computer answers to empty
/// heads: only from the operation that founded `fleet`.
fn start(fleet: &str, answer: &Value) -> Log {
    let mut ops = ops(answer);
    let found = ops
        .iter()
        .position(|op| matches!(op.body(), Body::Found(_)) && op.id().to_string() == fleet)
        .unwrap();
    let mut log = Log::found(ops.swap_remove(found)).unwrap();
    for (_, result) in log.insert_all(ops, now_ms()) {
        assert!(result.is_ok());
    }
    log
}

fn ops(answer: &Value) -> Vec<Op> {
    let texts: Vec<String> = serde_json::from_value(answer["ops"].clone()).unwrap();
    sync::decode_ops(&texts).unwrap()
}

fn heads(log: &Log) -> Vec<String> {
    log.heads().iter().map(ToString::to_string).collect()
}

#[tokio::test]
async fn a_phones_own_copy_signs_what_applies_on_the_computer() {
    let dir = tempfile::tempdir().unwrap();
    let state = founded(dir.path());
    let (_, token) = signed_in(&state, 1);
    let phone = Phone::new(7);
    admit(&state, &phone);
    let fleet = fleet_id(&state).to_string();
    let request = json!({ "fleet": fleet, "heads": [], "ops": [] });
    let (status, answer) = call(&state, &token, post("/api/v2/fleet/sync", request)).await;
    assert_eq!(status, StatusCode::OK, "{answer}");
    let mut copy = start(&fleet, &answer);
    assert!(copy.is_member(&phone.bundle.id()));
    assert_eq!(serde_json::to_value(heads(&copy)).unwrap(), answer["heads"]);
    // The computer's own host, from the phone's own copy.
    let me = state.fleet.as_ref().unwrap().me();
    let own = copy.state().members[&me]
        .bundle
        .host
        .as_ref()
        .unwrap()
        .id()
        .unwrap();
    // Drafted on the copy's own heads, checked before signing, then sealed.
    let link = Body::Link(Link {
        host: own,
        device_key: device_key(9),
    });
    let drafts = copy
        .drafts(phone.bundle.id(), now_ms(), vec![link], now_ms())
        .unwrap();
    let signed: Vec<Op> = drafts
        .into_iter()
        .map(|draft| draft.sign(&phone.key).unwrap())
        .collect();
    let computer_heads: Vec<OpId> = copy.heads().to_vec();
    for (_, result) in copy.insert_all(signed.clone(), now_ms()) {
        assert!(result.is_ok());
    }
    // What the computer lacks is the phone's own operation, which it takes.
    let missing = copy.missing_for(&computer_heads);
    assert_eq!(missing, signed.iter().collect::<Vec<_>>());
    let request = json!({
        "fleet": fleet, "heads": heads(&copy), "ops": sync::encode_ops(missing),
    });
    let (status, answer) = call(&state, &token, post("/api/v2/fleet/sync", request)).await;
    assert_eq!(status, StatusCode::OK, "{answer}");
    assert_eq!(answer["refused"], 0);
    assert_eq!(serde_json::to_value(heads(&copy)).unwrap(), answer["heads"]);
    let path = format!("/api/v2/fleet?member={}", phone.bundle.id());
    let (_, body) = call(&state, &token, get(&path)).await;
    assert_eq!(body["links"], json!([own.to_string()]));
}

#[tokio::test]
async fn a_sync_naming_another_fleet_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let state = founded(dir.path());
    let (_, token) = signed_in(&state, 1);
    let request = json!({ "fleet": "07".repeat(32), "heads": [], "ops": [] });
    let (status, _) = call(&state, &token, post("/api/v2/fleet/sync", request)).await;
    assert!(status.is_client_error(), "{status}");
}

#[tokio::test]
async fn a_removal_the_phone_pulls_stops_it_signing() {
    let dir = tempfile::tempdir().unwrap();
    let state = founded(dir.path());
    let (_, token) = signed_in(&state, 1);
    let phone = Phone::new(7);
    admit(&state, &phone);
    let fleet = fleet_id(&state).to_string();
    let request = json!({ "fleet": fleet, "heads": [], "ops": [] });
    let (_, answer) = call(&state, &token, post("/api/v2/fleet/sync", request)).await;
    let mut copy = start(&fleet, &answer);
    // A person removes the phone in Sheprd.
    let removal = Ask::Remove {
        member: phone.bundle.id().to_string(),
        also_members: Vec::new(),
        also_hosts: Vec::new(),
        reason: Reason::Lost,
    };
    assert_eq!(ask(&state, removal), Answer::Done);
    let request = json!({ "fleet": fleet, "heads": heads(&copy), "ops": [] });
    let (_, answer) = call(&state, &token, post("/api/v2/fleet/sync", request)).await;
    for (_, result) in copy.insert_all(ops(&answer), now_ms()) {
        assert!(result.is_ok());
    }
    assert!(!copy.is_member(&phone.bundle.id()));
    let me = state.fleet.as_ref().unwrap().me();
    let body = Body::Remove(Remove {
        member: me,
        also_members: Vec::new(),
        also_hosts: Vec::new(),
        reason: Reason::Compromised,
    });
    assert_eq!(
        copy.drafts(phone.bundle.id(), now_ms(), vec![body], now_ms()),
        Err(crate::control::fleet::Error::Signer)
    );
}
