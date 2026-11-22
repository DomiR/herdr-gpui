//! Signing from the phone's own copy: drafts on its own heads, refused
//! before Face ID when its replay would refuse them, sealed only when
//! they apply, and handed to a computer as what it lacks.

use super::*;

fn draft(fleet: &Fleet, log: &str, bodies: Value) -> Value {
    json!({
        "call": "draft", "fleet": fleet.fleet(), "me": fleet.p.hex(), "log": log,
        "issued_ms": NOW, "now_ms": NOW, "bodies": bodies,
    })
}

fn seal(fleet: &Fleet, log: &str, ops: &[String]) -> Value {
    json!({
        "call": "seal", "fleet": fleet.fleet(), "me": fleet.p.hex(), "log": log,
        "ops": ops, "now_ms": NOW,
    })
}

fn remove(member: &Device) -> Value {
    json!({ "body": "remove", "member": member.hex(), "reason": "lost" })
}

#[test]
fn drafts_are_signed_sealed_and_handed_over() {
    let fleet = Fleet::new();
    let replica = fleet.replica();
    let log = log_of(&replica);
    let link = json!({
        "body": "link", "host": fleet.host,
        "device_key": sync::text(&uncompressed(&signing_key(70))),
    });
    let drafted = ok(&draft(&fleet, &log, json!([link, remove(&fleet.b)])));
    let drafts = drafted["drafts"].as_array().unwrap();
    assert_eq!(drafts[0]["seq"], 1);
    assert_eq!(drafts[1]["seq"], 2);
    assert_eq!(drafts[1]["parents"], json!([drafts[0]["id"]]));
    let signed = sign_drafts(&drafted, &fleet.p.key);
    let sealed = ok(&seal(&fleet, &log, &signed));
    assert_eq!(sealed["applied"], 2);
    let status = &sealed["status"];
    assert_eq!(status["links"], json!([fleet.host]));
    assert_eq!(status["next"]["seq"], 3);
    assert!(
        !status["view"]["members"]
            .as_array()
            .unwrap()
            .iter()
            .any(|member| member["id"] == fleet.b.hex().as_str())
    );
    // A computer at the old heads lacks exactly these two, which it takes.
    let heads: Vec<String> = fleet.log.heads().iter().map(ToString::to_string).collect();
    let missing = ok(&json!({
        "call": "missing", "fleet": fleet.fleet(), "log": log_of(&sealed),
        "heads": heads, "now_ms": NOW,
    }));
    assert_eq!(missing["ops"], json!(signed));
    assert_eq!(missing["more"], false);
    let mut computer = fleet.log.clone();
    let ops = sync::decode_ops(&signed).unwrap();
    assert!(
        computer
            .insert_all(ops, NOW)
            .iter()
            .all(|(_, result)| result.is_ok())
    );
    assert!(!computer.is_member(&fleet.b.id()));
}

#[test]
fn the_phone_refuses_before_signing_what_its_replay_refuses() {
    let mut fleet = Fleet::new();
    // A member no longer in the fleet.
    fleet.remove(&fleet.a.clone(), &fleet.b.clone());
    let log = log_of(&fleet.replica());
    let error = refused(&draft(&fleet, &log, json!([remove(&fleet.b)])));
    assert_eq!(error["rule"], "unknown");
    // A phone that was removed signs nothing at all.
    fleet.remove(&fleet.a.clone(), &fleet.p.clone());
    let log = log_of(&fleet.replica());
    let host = json!({ "body": "host_remove", "host": fleet.host });
    let error = refused(&draft(&fleet, &log, json!([host])));
    assert_eq!(error["rule"], "signer");
}

#[test]
fn a_draft_names_one_body_to_one_per_host_the_fleet_holds() {
    let fleet = Fleet::new();
    let log = log_of(&fleet.replica());
    assert_eq!(refused(&draft(&fleet, &log, json!([])))["kind"], "bounds");
    let host = json!({ "body": "host_remove", "host": fleet.host });
    let many = vec![host; MAX_BODIES + 1];
    assert_eq!(refused(&draft(&fleet, &log, json!(many)))["kind"], "bounds");
}

#[test]
fn a_link_to_every_host_the_fleet_holds_is_one_draft() {
    let fleet = Fleet::new();
    let log = log_of(&fleet.replica());
    let links: Vec<Value> = (0..MAX_BODIES)
        .map(|n| {
            let mut seed = [7u8; 32];
            seed[30..].copy_from_slice(&u16::try_from(n).unwrap().to_be_bytes());
            let key = SigningKey::from_slice(&seed).unwrap();
            json!({ "body": "link", "host": fleet.host, "device_key": sync::text(&uncompressed(&key)) })
        })
        .collect();
    let drafted = ok(&draft(&fleet, &log, json!(links)));
    assert_eq!(drafted["drafts"].as_array().unwrap().len(), MAX_BODIES);
    let sealed = ok(&seal(&fleet, &log, &sign_drafts(&drafted, &fleet.p.key)));
    assert_eq!(sealed["applied"], MAX_BODIES);
}

#[test]
fn only_what_this_phone_signed_and_applies_is_sealed() {
    let fleet = Fleet::new();
    let log = log_of(&fleet.replica());
    let drafted = ok(&draft(&fleet, &log, json!([remove(&fleet.b)])));
    // Signed by another key in the phone's name.
    let forged = sign_drafts(&drafted, &fleet.b.key);
    assert_eq!(refused(&seal(&fleet, &log, &forged))["rule"], "signature");
    // Another member's operation is not the phone's to seal.
    let theirs = fleet.log.draft(
        fleet.a.id(),
        NOW,
        Body::HostRemove(sheprd_remote_control::control::fleet::HostId([7; 32])),
    );
    let theirs = sync::encode_ops([&theirs.sign(&fleet.a.key).unwrap()]);
    assert_eq!(refused(&seal(&fleet, &log, &theirs))["kind"], "not_mine");
    assert_eq!(refused(&seal(&fleet, &log, &[]))["kind"], "bounds");
}

#[test]
fn what_a_computer_lacks_comes_one_sync_at_a_time() {
    let mut fleet = Fleet::new();
    let a = fleet.a.clone();
    for seed in 0..70u8 {
        let body = Body::HostRemove(sheprd_remote_control::control::fleet::HostId([seed; 32]));
        let op = act(&mut fleet.log, &a, body);
        fleet.ops.push(op);
    }
    let log = log_of(&fleet.replica());
    let missing = ok(&json!({
        "call": "missing", "fleet": fleet.fleet(), "log": log, "heads": [], "now_ms": NOW,
    }));
    assert_eq!(missing["ops"].as_array().unwrap().len(), MAX_SYNC_OPS);
    assert_eq!(missing["more"], true);
}
