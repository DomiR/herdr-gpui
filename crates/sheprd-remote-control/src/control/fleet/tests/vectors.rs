//! The vectors the iOS app's Swift copy is checked against: one signed
//! operation of every body, a joining member's signature, a host's signed
//! self-record, a state with its hash, and operations that break a rule
//! of their fields, which both must refuse. Keys are fixed and P-256
//! signatures deterministic (RFC 6979), so the bytes never move unless the
//! encoding does. `HERDR_UPDATE_FIXTURES=1` rewrites them.

use super::*;
use serde_json::{Value, json};
use std::path::PathBuf;

fn hex(bytes: &[u8]) -> String {
    super::super::hex(bytes)
}

fn check(name: &str, value: &Value) {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../ios/SheprdRemoteCore/Tests/SheprdRemoteCoreTests/Fixtures/fleet")
        .join(format!("{name}.json"));
    let text = format!("{}\n", serde_json::to_string_pretty(value).unwrap());
    if std::env::var_os("HERDR_UPDATE_FIXTURES").is_some() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, text).unwrap();
        return;
    }
    let expected = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    let expected: Value = serde_json::from_str(&expected).unwrap();
    assert_eq!(
        value,
        &expected,
        "{} is out of date; run with HERDR_UPDATE_FIXTURES=1",
        path.display()
    );
}

fn body_name(body: &Body) -> &'static str {
    match body {
        Body::Found(_) => "found",
        Body::Admit(_) => "admit",
        Body::Remove(_) => "remove",
        Body::Rotate(_) => "rotate",
        Body::HostAdd(_) => "host_add",
        Body::Access(_) => "access",
        Body::HostSeen(_) => "host_seen",
        Body::HostRemove(_) => "host_remove",
        Body::Link(_) => "link",
        Body::Checkpoint(_) => "checkpoint",
    }
}

#[test]
fn op_vectors() {
    let ops = encoding::every_body();
    let keys: std::collections::BTreeMap<MemberId, body::PublicKey> =
        [computer(1), computer(2), phone(3)]
            .iter()
            .map(|device| (device.id(), device.bundle.key))
            .collect();
    let names: std::collections::BTreeSet<&str> =
        ops.iter().map(|op| body_name(op.body())).collect();
    assert_eq!(names.len(), 10, "every body has a vector");
    let vectors: Vec<Value> = ops
        .iter()
        .map(|op| {
            json!({
                "body": body_name(op.body()),
                "op": hex(&op.encode()),
                "id": hex(&op.id().0),
                "signer": hex(&op.signer().0),
                "signer_key": hex(&keys[&op.signer()]),
                "seq": op.unsigned().seq,
                "parents": op.parents().iter().map(|parent| hex(&parent.0)).collect::<Vec<_>>(),
            })
        })
        .collect();
    check(
        "ops",
        &json!({ "fleet": hex(&ops[1].unsigned().fleet.0), "ops": vectors }),
    );
}

#[test]
fn join_and_self_record_vectors() {
    let a = computer(1);
    let log = found(&a);
    let joiner = phone(3);
    let message = joiner.bundle.join_message(&log.fleet()).unwrap();
    let (intro, ca) = ca_host(40, HostKind::Sheprd);
    let record = intro.record.message().unwrap();
    check(
        "signed",
        &json!({
            "join": {
                "fleet": hex(&log.fleet().0),
                "key": hex(&joiner.bundle.key),
                "message": hex(&message),
                "signature": hex(&raw_signature(&joiner.key, &message)),
            },
            "self_record": {
                "host": hex(&intro.id().unwrap().0),
                "certificate": hex(&intro.ca.as_ref().unwrap().certificate),
                "key": hex(&uncompressed(&ca)),
                "message": hex(&record),
                "signature": hex(&intro.record_signature.unwrap()),
            },
            "plain_host": {
                "first_host_key": plain_host(41).record.ssh_host_keys[0],
                "id": hex(&plain_host(41).id().unwrap().0),
            },
        }),
    );
}

#[test]
fn state_vector() {
    let ops = encoding::every_body();
    let mut log = Log::found(ops[0].clone()).unwrap();
    for (_, result) in log.insert_all(ops[1..].to_vec(), NOW) {
        assert_eq!(result, Ok(Inserted::Applied));
    }
    let bytes = log.state().encode().unwrap();
    check(
        "state",
        &json!({ "state": hex(&bytes), "hash": hex(&log.state().hash().unwrap()) }),
    );
}

/// Replaces the first `find` in `bytes` with `replace`.
fn patched(bytes: &[u8], find: &[u8], replace: &[u8]) -> Vec<u8> {
    let at = bytes
        .windows(find.len())
        .position(|window| window == find)
        .unwrap();
    [&bytes[..at], replace, &bytes[at + find.len()..]].concat()
}

/// Operations that read byte for byte but break a rule of their fields or
/// shape: the Swift copy must refuse every one as the daemon does.
#[test]
fn invalid_vectors() {
    let ops: Vec<Vec<u8>> = encoding::every_body().iter().map(Op::encode).collect();
    let phone_key = phone(3).bundle.key;
    let host = ca_host(40, HostKind::Sheprd).0.id().unwrap();
    let device_key = compressed(&signing_key(70));
    let mut invalid_point = [0xff; 33];
    invalid_point[0] = 0x02;
    let mut other_host = host.0;
    other_host[0] ^= 1;
    let parents_at = DOMAIN.len() + 1 + 32 + 32 + 16;
    let mut found_with_fleet = ops[0].clone();
    found_with_fleet[DOMAIN.len() + 1..DOMAIN.len() + 33].fill(1);
    let mut duplicate_parents = ops[4].clone();
    assert_eq!(duplicate_parents[parents_at], 1);
    duplicate_parents[parents_at] = 2;
    let parent = duplicate_parents[parents_at + 1..parents_at + 33].to_vec();
    duplicate_parents.splice(parents_at + 1..parents_at + 1, parent);
    let cases = [
        (
            "phone_as_computer",
            patched(
                &ops[2],
                &[&phone_key[..], &[2]].concat(),
                &[&phone_key[..], &[1]].concat(),
            ),
        ),
        (
            "blank_name",
            patched(&ops[2], b"\x00\x06phone3", b"\x00\x06      "),
        ),
        ("control_in_name", patched(&ops[2], b"phone3", b"phon\x073")),
        ("invalid_key", patched(&ops[2], &phone_key, &invalid_point)),
        ("found_with_fleet", found_with_fleet),
        ("duplicate_parents", duplicate_parents),
        (
            "bad_ssh_key",
            patched(&ops[8], b"ssh-ed25519 A", b"ssh-ed25519 !"),
        ),
        (
            "record_for_another_host",
            patched(&ops[3], &host.0, &other_host),
        ),
        (
            "bad_device_key",
            patched(&ops[6], &device_key, &invalid_point),
        ),
        ("space_in_relay", patched(&ops[5], b"relay41", b"rela 41")),
    ];
    for (name, bytes) in &cases {
        assert!(Op::decode(bytes).is_err(), "{name} decodes");
    }
    let vectors: Vec<Value> = cases
        .iter()
        .map(|(name, bytes)| json!({ "case": name, "op": hex(bytes) }))
        .collect();
    check("invalid", &json!({ "ops": vectors }));
}
