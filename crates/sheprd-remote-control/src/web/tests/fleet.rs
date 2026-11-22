//! `/api/v2/fleet`: a phone reads the fleet, posts the operations it
//! signs, and asks to join, which only a person's approval in Sheprd
//! answers. Unix only, as the fleet's own tests: a computer's bundle names
//! its user from `USER`.

use super::*;
use crate::control::fleet::{
    Access, Body, Bundle, FleetId, Kind, Link, Log, Method, OpId, Reason, Remove, Unsigned,
    sync::{self, Answer, Ask},
};
use p256::ecdsa::{Signature, SigningKey, signature::Signer};
use serde_json::{Value, json};

/// A phone: its fleet key and its bundle.
pub(crate) struct Phone {
    pub(crate) key: SigningKey,
    pub(crate) bundle: Bundle,
}

impl Phone {
    pub(crate) fn new(seed: u8) -> Self {
        let key = SigningKey::from_slice(&[seed; 32]).unwrap();
        let dial = SigningKey::from_slice(&[seed.wrapping_add(100); 32]).unwrap();
        let bundle = Bundle {
            key: point(&key),
            kind: Kind::Phone,
            name: format!("phone{seed}"),
            dial: point(&dial),
            tunnel: [seed; 32],
            ssh: None,
            host: None,
        };
        Self { key, bundle }
    }

    /// Its bundle and join signature for `fleet`, base64url.
    pub(crate) fn join(&self, fleet: &FleetId) -> (String, String) {
        let message = self.bundle.join_message(fleet).unwrap();
        let signature: Signature = self.key.sign(&message);
        (
            sync::text(&self.bundle.to_bytes().unwrap()),
            sync::text(&signature.to_bytes()),
        )
    }

    /// Signs `body` on what `log` holds.
    pub(crate) fn sign(&self, log: &Log, body: Body) -> crate::control::fleet::Op {
        log.draft(self.bundle.id(), now_ms(), body)
            .sign(&self.key)
            .unwrap()
    }
}

fn point(key: &SigningKey) -> [u8; 33] {
    key.verifying_key()
        .to_sec1_point(true)
        .as_bytes()
        .try_into()
        .unwrap()
}

/// A device key a phone makes for one host, SEC1 uncompressed.
pub(crate) fn device_key(seed: u8) -> Vec<u8> {
    SigningKey::from_slice(&[seed; 32])
        .unwrap()
        .verifying_key()
        .to_sec1_point(false)
        .as_bytes()
        .to_vec()
}

/// A daemon's state that founded a fleet, with a relay configured (not
/// dialed) and SSH host keys, as its bundle needs.
pub(crate) fn founded(dir: &std::path::Path) -> Shared {
    let mut state = state(dir);
    let endpoint = sheprd_relay::Endpoint::parse("http://127.0.0.1:9").unwrap();
    let relays = Arc::new(crate::relay::Relays::load(dir, vec![endpoint.into()]).unwrap());
    let fleet = Arc::new(crate::fleet::Fleet::load(dir, now_ms()).unwrap());
    {
        let state = Arc::get_mut(&mut state).unwrap();
        state.relay = Some(relays);
        state.fleet = Some(Arc::clone(&fleet));
    }
    let host_key_dir = dir.join("etc-ssh");
    std::fs::create_dir(&host_key_dir).unwrap();
    std::fs::write(
        host_key_dir.join("ssh_host_ed25519_key.pub"),
        format!(
            "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAI{} root@node\n",
            "A".repeat(43)
        ),
    )
    .unwrap();
    state.auth().set_ssh_access(crate::ssh_access::SshAccess {
        host_key_dir,
        keys: crate::ssh_access::AuthorizedKeys::at(dir.join("authorized_keys")),
        server: "127.0.0.1:22".parse().unwrap(),
    });
    assert_eq!(ask(&state, Ask::Found), Answer::Done);
    state
}

pub(crate) fn ask(state: &Shared, ask: Ask) -> Answer {
    crate::fleet::ask::answer(state, state.fleet.as_ref().unwrap(), ask, now_ms()).unwrap()
}

pub(crate) fn fleet_id(state: &Shared) -> FleetId {
    state
        .fleet
        .as_ref()
        .unwrap()
        .inner()
        .log
        .as_ref()
        .unwrap()
        .fleet()
}

fn with_bearer(token: &str, mut request: HttpRequest<Body_>) -> HttpRequest<Body_> {
    request.headers_mut().insert(
        header::AUTHORIZATION,
        HeaderValue::from_str(&format!("Bearer {token}")).unwrap(),
    );
    request
}

use axum::body::Body as Body_;

pub(crate) async fn call(
    state: &Shared,
    token: &str,
    request: HttpRequest<Body_>,
) -> (StatusCode, Value) {
    let (status, _, body) = send(state, with_bearer(token, request)).await;
    (status, serde_json::from_str(&body).unwrap_or(Value::Null))
}

/// Admits `phone` as the person would in Sheprd.
pub(crate) fn admit(state: &Shared, phone: &Phone) {
    let (bundle, signature) = phone.join(&fleet_id(state));
    assert_eq!(
        ask(
            state,
            Ask::Admit {
                bundle,
                signature,
                method: Method::Qr,
            }
        ),
        Answer::Done
    );
}

#[tokio::test]
async fn the_fleet_needs_a_session() {
    let dir = tempfile::tempdir().unwrap();
    let state = founded(dir.path());
    assert_eq!(
        send(&state, get("/api/v2/fleet")).await.0,
        StatusCode::UNAUTHORIZED
    );
    let sync = post("/api/v2/fleet/sync", json!({"heads": [], "ops": []}));
    assert_eq!(send(&state, sync).await.0, StatusCode::UNAUTHORIZED);
    let (bundle, signature) = Phone::new(7).join(&fleet_id(&state));
    let join = post(
        "/api/v2/fleet/join",
        json!({"bundle": bundle, "signature": signature}),
    );
    assert_eq!(send(&state, join).await.0, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn a_computer_in_no_fleet_says_so() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = state(dir.path());
    let (_, token) = signed_in(&state, 1);
    // No fleet key at all.
    assert_eq!(
        call(&state, &token, get("/api/v2/fleet")).await.0,
        StatusCode::NOT_FOUND
    );
    // A key, and no fleet yet.
    Arc::get_mut(&mut state).unwrap().fleet = Some(Arc::new(
        crate::fleet::Fleet::load(dir.path(), now_ms()).unwrap(),
    ));
    let (status, body) = call(&state, &token, get("/api/v2/fleet")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, json!({"heads": []}));
    let sync = post("/api/v2/fleet/sync", json!({"heads": [], "ops": []}));
    assert_eq!(call(&state, &token, sync).await.0, StatusCode::CONFLICT);
}

#[tokio::test]
async fn a_phone_reads_the_fleet_and_what_it_signs_next() {
    let dir = tempfile::tempdir().unwrap();
    let state = founded(dir.path());
    let (_, token) = signed_in(&state, 1);
    let phone = Phone::new(7);
    let member = phone.bundle.id().to_string();
    let (status, body) = call(&state, &token, get("/api/v2/fleet")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["view"]["members"].as_array().unwrap().len(), 1);
    assert!(body.get("next").is_none());
    admit(&state, &phone);
    let path = format!("/api/v2/fleet?member={member}");
    let (_, body) = call(&state, &token, get(&path)).await;
    assert_eq!(body["join"], "member");
    assert_eq!(body["next"]["seq"], 1);
    assert_eq!(body["next"]["fleet"], fleet_id(&state).to_string());
    assert_eq!(body["next"]["parents"], body["heads"]);
}

#[tokio::test]
async fn a_phones_link_and_removal_go_through_sync() {
    let dir = tempfile::tempdir().unwrap();
    let state = founded(dir.path());
    let (_, token) = signed_in(&state, 1);
    let phone = Phone::new(7);
    admit(&state, &phone);
    let fleet = state.fleet.clone().unwrap();
    let own = crate::fleet::grants::own_host(
        fleet.inner().log.as_ref().unwrap().state(),
        &fleet.me(),
        None,
    )
    .unwrap();
    // The phone builds its operation from what the computer answered.
    let path = format!("/api/v2/fleet?member={}", phone.bundle.id());
    let (_, body) = call(&state, &token, get(&path)).await;
    // Nothing linked yet: no routes.
    assert!(body.get("routes").is_none());
    let next = &body["next"];
    let link = Unsigned {
        fleet: sync::fleet(next["fleet"].as_str().unwrap()).unwrap(),
        signer: phone.bundle.id(),
        seq: next["seq"].as_u64().unwrap(),
        issued_ms: now_ms(),
        parents: next["parents"]
            .as_array()
            .unwrap()
            .iter()
            .map(|parent| OpId::from_hex(parent.as_str().unwrap()).unwrap())
            .collect(),
        body: Body::Link(Link {
            host: own,
            device_key: device_key(9),
        }),
    }
    .sign(&phone.key)
    .unwrap();
    let request = post(
        "/api/v2/fleet/sync",
        json!({"heads": body["heads"], "ops": sync::encode_ops([&link])}),
    );
    let (status, answer) = call(&state, &token, request).await;
    assert_eq!(status, StatusCode::OK, "{answer}");
    assert_eq!(answer["refused"], 0);
    assert_eq!(answer["heads"], json!([link.id().to_string()]));
    // The heads it sent predate its own operation, so it comes back too.
    assert_eq!(answer["ops"], json!(sync::encode_ops([&link])));
    let (_, body) = call(&state, &token, get(&path)).await;
    assert_eq!(body["links"], json!([own.to_string()]));
    // With the device key each link names, for the phone to check its own.
    assert_eq!(
        body["link_keys"],
        json!({ own.to_string(): sync::text(&device_key(9)) })
    );
    // The linked host's record as its CA signed it, which the phone checks
    // against the fingerprint it pins: the certificate hashes to the host
    // id, and its key signed the record.
    let record = fleet.inner().log.as_ref().unwrap().state().hosts[&own]
        .record
        .clone();
    let routes = body["routes"].as_array().unwrap();
    assert_eq!(routes.len(), 1);
    let route = &routes[0];
    assert_eq!(route["host"], own.to_string());
    let bytes = |field: &str| sync::bytes(route[field].as_str().unwrap()).unwrap();
    use sha2::Digest as _;
    assert_eq!(sha2::Sha256::digest(bytes("certificate")).as_slice(), own.0);
    assert_eq!(bytes("record"), record.message().unwrap());
    let key = p256::ecdsa::VerifyingKey::from_sec1_bytes(&bytes("key")).unwrap();
    let signature = Signature::from_slice(&bytes("signature")).unwrap();
    p256::ecdsa::signature::Verifier::verify(&key, &bytes("record"), &signature).unwrap();
    // A phone may not grant access, and its removal of the computer that
    // founded the fleet is a removal like any member's.
    let log = fleet.inner().log.as_ref().map(|log| {
        (
            phone.sign(
                log,
                Body::Access(Access {
                    host: own,
                    member: fleet.me(),
                    allow: true,
                    rev: 1,
                }),
            ),
            phone.sign(
                log,
                Body::Remove(Remove {
                    member: fleet.me(),
                    also_members: Vec::new(),
                    also_hosts: Vec::new(),
                    reason: Reason::Lost,
                }),
            ),
        )
    });
    let (access, remove) = log.unwrap();
    let request = post(
        "/api/v2/fleet/sync",
        json!({"ops": sync::encode_ops([&access])}),
    );
    assert_eq!(call(&state, &token, request).await.1["refused"], 1);
    let request = post(
        "/api/v2/fleet/sync",
        json!({"ops": sync::encode_ops([&remove])}),
    );
    assert_eq!(call(&state, &token, request).await.1["refused"], 0);
    assert!(!fleet.inner().log.as_ref().unwrap().is_member(&fleet.me()));
}

/// What the phone confirms a post by: an operation that waits for its
/// parents here is taken without a refusal, but the member's next
/// sequence number moves only once it applies.
#[tokio::test]
async fn a_waiting_operation_does_not_move_the_next_sequence_number() {
    let dir = tempfile::tempdir().unwrap();
    let state = founded(dir.path());
    let (_, token) = signed_in(&state, 1);
    let phone = Phone::new(7);
    admit(&state, &phone);
    let path = format!("/api/v2/fleet?member={}", phone.bundle.id());
    let (_, body) = call(&state, &token, get(&path)).await;
    let seq = body["next"]["seq"].as_u64().unwrap();
    // On a parent this computer has not seen.
    let waiting = Unsigned {
        fleet: fleet_id(&state),
        signer: phone.bundle.id(),
        seq,
        issued_ms: now_ms(),
        parents: vec![OpId([5; 32])],
        body: Body::HostRemove(crate::control::fleet::HostId([4; 32])),
    }
    .sign(&phone.key)
    .unwrap();
    let request = post(
        "/api/v2/fleet/sync",
        json!({ "ops": sync::encode_ops([&waiting]) }),
    );
    let (status, answer) = call(&state, &token, request).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(answer["refused"], 0);
    let (_, body) = call(&state, &token, get(&path)).await;
    assert_eq!(body["next"]["seq"].as_u64().unwrap(), seq);
}

#[tokio::test]
async fn a_sync_over_its_bounds_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let state = founded(dir.path());
    let (_, token) = signed_in(&state, 1);
    let ops = vec!["AA"; super::super::fleet::MAX_SYNC_OPS + 1];
    let request = post("/api/v2/fleet/sync", json!({ "ops": ops }));
    assert_eq!(
        call(&state, &token, request).await.0,
        StatusCode::BAD_REQUEST
    );
    let heads = vec!["00".repeat(32); super::super::fleet::MAX_HEADS + 1];
    let request = post("/api/v2/fleet/sync", json!({ "heads": heads }));
    assert_eq!(
        call(&state, &token, request).await.0,
        StatusCode::BAD_REQUEST
    );
    let request = post("/api/v2/fleet/sync", json!({ "ops": ["not an op"] }));
    assert_eq!(
        call(&state, &token, request).await.0,
        StatusCode::BAD_REQUEST
    );
    let request = post(
        "/api/v2/fleet/sync",
        json!({ "fleet": "11".repeat(32), "ops": [] }),
    );
    assert_eq!(
        call(&state, &token, request).await.0,
        StatusCode::BAD_REQUEST
    );
    let huge = "A".repeat(super::super::fleet::MAX_SYNC_BODY);
    let request = post("/api/v2/fleet/sync", json!({ "ops": [huge] }));
    assert_eq!(
        call(&state, &token, request).await.0,
        StatusCode::PAYLOAD_TOO_LARGE
    );
}

#[tokio::test]
async fn a_join_waits_for_a_person() {
    let dir = tempfile::tempdir().unwrap();
    let state = founded(dir.path());
    let (_, token) = signed_in(&state, 1);
    let phone = Phone::new(7);
    let (bundle, signature) = phone.join(&fleet_id(&state));
    let request = post(
        "/api/v2/fleet/join",
        json!({"bundle": bundle, "signature": signature}),
    );
    let (status, body) = call(&state, &token, request).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        body,
        json!({"member": phone.bundle.id().to_string(), "state": "pending"})
    );
    let path = format!("/api/v2/fleet?member={}", phone.bundle.id());
    let (_, read) = call(&state, &token, get(&path)).await;
    assert_eq!(read["join"], "pending");
    assert!(read.get("next").is_none());
    let fleet = state.fleet.clone().unwrap();
    assert!(
        !fleet
            .inner()
            .log
            .as_ref()
            .unwrap()
            .is_member(&phone.bundle.id())
    );
    // Sheprd lists it, with the device that asked, and admits it only
    // when asked to.
    let Answer::Joins { joins } = ask(&state, Ask::Joins) else {
        panic!("no joins");
    };
    assert_eq!(joins.len(), 1);
    assert_eq!(joins[0].device, "iPhone");
    assert_eq!(joins[0].kind, Kind::Phone);
    assert_eq!(
        ask(
            &state,
            Ask::Admit {
                bundle: joins[0].bundle.clone(),
                signature: joins[0].signature.clone(),
                method: Method::Qr,
            }
        ),
        Answer::Done
    );
    let Answer::Joins { joins } = ask(&state, Ask::Joins) else {
        panic!("no joins");
    };
    assert!(joins.is_empty());
    let (_, read) = call(&state, &token, get(&path)).await;
    assert_eq!(read["join"], "member");
}

#[tokio::test]
async fn a_join_must_be_a_phone_that_signed_it() {
    let dir = tempfile::tempdir().unwrap();
    let state = founded(dir.path());
    let (_, token) = signed_in(&state, 1);
    let phone = Phone::new(7);
    let (bundle, _) = phone.join(&fleet_id(&state));
    let (_, other) = Phone::new(8).join(&fleet_id(&state));
    let request = post(
        "/api/v2/fleet/join",
        json!({"bundle": bundle, "signature": other}),
    );
    assert_eq!(
        call(&state, &token, request).await.0,
        StatusCode::BAD_REQUEST
    );
    // Signed for another fleet.
    let (_, elsewhere) = phone.join(&FleetId([3; 32]));
    let request = post(
        "/api/v2/fleet/join",
        json!({"bundle": bundle, "signature": elsewhere}),
    );
    assert_eq!(
        call(&state, &token, request).await.0,
        StatusCode::BAD_REQUEST
    );
    let request = post(
        "/api/v2/fleet/join",
        json!({"bundle": "AA", "signature": "AA"}),
    );
    assert_eq!(
        call(&state, &token, request).await.0,
        StatusCode::BAD_REQUEST
    );
}

#[tokio::test]
async fn joins_are_bounded() {
    let dir = tempfile::tempdir().unwrap();
    let state = founded(dir.path());
    let fleet = state.fleet.clone().unwrap();
    let id = fleet_id(&state);
    let max = crate::fleet::joins::MAX;
    for seed in 0..=max {
        let phone = Phone::new(10 + u8::try_from(seed).unwrap());
        let (_, signature) = phone.join(&id);
        let signature = sync::bytes(&signature).unwrap().try_into().unwrap();
        let inner = fleet.inner();
        let result = fleet.joins().ask(
            inner.log.as_ref().unwrap(),
            phone.bundle.clone(),
            signature,
            (DeviceId(format!("device{seed}")), "iPhone".into()),
            now_ms(),
        );
        if seed < max {
            assert!(result.is_ok(), "{result:?}");
        } else {
            assert!(matches!(result, Err(crate::fleet::joins::Error::Full)));
        }
    }
    // Ten minutes on, they are gone.
    let paired: std::collections::HashSet<DeviceId> = (0..=max)
        .map(|seed| DeviceId(format!("device{seed}")))
        .collect();
    assert_eq!(fleet.joins().list(now_ms(), &paired).len(), max);
    assert!(
        fleet
            .joins()
            .list(now_ms() + crate::fleet::joins::TTL_MS, &paired)
            .is_empty()
    );
    // One device keeps one join: a second replaces its first.
    let (_, token) = signed_in(&state, 1);
    for seed in [30, 31] {
        let (bundle, signature) = Phone::new(seed).join(&id);
        let request = post(
            "/api/v2/fleet/join",
            json!({"bundle": bundle, "signature": signature}),
        );
        assert_eq!(call(&state, &token, request).await.0, StatusCode::OK);
    }
    let Answer::Joins { joins } = ask(&state, Ask::Joins) else {
        panic!("no joins");
    };
    assert_eq!(joins.len(), 1);
}

/// The vector the iOS app's `FleetClient` is checked against: a `Link`
/// built from what `GET /api/v2/fleet` answers, its signed bytes, and the
/// device id the link's key gets. `HERDR_UPDATE_FIXTURES=1` rewrites it.
#[test]
fn phone_vector() {
    let phone = Phone::new(7);
    let device_key = device_key(20);
    let host = crate::control::fleet::HostId([4; 32]);
    let unsigned = Unsigned {
        fleet: FleetId([9; 32]),
        signer: phone.bundle.id(),
        seq: 3,
        issued_ms: 1_800_000_000_000,
        parents: vec![OpId([1; 32]), OpId([2; 32])],
        body: Body::Link(Link {
            host,
            device_key: device_key.clone(),
        }),
    };
    let signed = unsigned.encode().unwrap();
    let op = unsigned.sign(&phone.key).unwrap();
    let (device, _) = crate::fleet::links::device_id(&device_key).unwrap();
    let hex = |bytes: &[u8]| -> String { bytes.iter().map(|byte| format!("{byte:02x}")).collect() };
    let value = json!({
        "next": {
            "fleet": FleetId([9; 32]).to_string(),
            "seq": 3,
            "parents": [OpId([1; 32]).to_string(), OpId([2; 32]).to_string()],
        },
        "signer": phone.bundle.id().to_string(),
        "signer_key": hex(&phone.bundle.key),
        "issued_ms": 1_800_000_000_000_u64,
        "host": host.to_string(),
        "device_key": hex(&device_key),
        "device": device.0,
        "signed": hex(&signed),
        "op": sync::text(&op.encode()),
    });
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../ios/SheprdRemoteCore/Tests/SheprdRemoteCoreTests/Fixtures/fleet/phone.json");
    let text = format!("{}\n", serde_json::to_string_pretty(&value).unwrap());
    if std::env::var_os("HERDR_UPDATE_FIXTURES").is_some() {
        std::fs::write(&path, text).unwrap();
        return;
    }
    let expected: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(
        value,
        expected,
        "{} is out of date; run with HERDR_UPDATE_FIXTURES=1",
        path.display()
    );
}
