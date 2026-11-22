//! The phone's copy: started only from the fleet it joined, every
//! operation checked as a computer checks it, and the saved bytes checked
//! again when read back.

use super::*;

#[test]
fn a_replica_starts_from_the_fleet_it_joined_and_says_what_the_phone_signs_on() {
    let fleet = Fleet::new();
    let replica = fleet.replica();
    assert_eq!(replica["applied"], fleet.ops.len() - 1);
    assert_eq!(replica["refused"], 0);
    let status = &replica["status"];
    assert_eq!(status["join"], "member");
    assert_eq!(status["removed"], false);
    let members: Vec<&str> = status["view"]["members"]
        .as_array()
        .unwrap()
        .iter()
        .map(|member| member["id"].as_str().unwrap())
        .collect();
    for device in [&fleet.a, &fleet.b, &fleet.p] {
        assert!(members.contains(&device.hex().as_str()));
    }
    assert!(
        status["view"]["hosts"]
            .as_array()
            .unwrap()
            .iter()
            .any(|host| host["id"] == fleet.host.as_str())
    );
    // The phone signed nothing yet: its first operation, on the heads.
    let heads: Vec<String> = fleet.log.heads().iter().map(ToString::to_string).collect();
    assert_eq!(status["next"]["seq"], 1);
    assert_eq!(status["next"]["parents"], json!(heads));
    assert_eq!(status["heads"], json!(heads));
}

#[test]
fn another_fleet_cannot_be_handed_to_the_phone() {
    // A computer of another fleet answers with its own history.
    let fleet = Fleet::new();
    let stranger = computer(5);
    let theirs = Unsigned {
        fleet: FleetId([0; 32]),
        signer: stranger.id(),
        seq: 1,
        issued_ms: NOW,
        parents: Vec::new(),
        body: Body::Found(stranger.bundle.clone()),
    }
    .sign(&stranger.key)
    .unwrap();
    let ops = sync::encode_ops([&theirs]);
    let error = refused(&json!({
        "call": "apply", "fleet": fleet.fleet(), "me": fleet.p.hex(),
        "ops": ops, "now_ms": NOW,
    }));
    assert_eq!(error["kind"], "unfounded");
    // Nor can its saved log be opened as this fleet's.
    let their_replica = ok(&json!({
        "call": "apply", "fleet": theirs.id().to_string(), "me": fleet.p.hex(),
        "ops": ops, "now_ms": NOW,
    }));
    let error = refused(&json!({
        "call": "apply", "fleet": fleet.fleet(), "me": fleet.p.hex(),
        "log": log_of(&their_replica), "now_ms": NOW,
    }));
    assert_eq!(error["kind"], "other_fleet");
}

#[test]
fn history_without_its_founding_operation_starts_nothing() {
    let fleet = Fleet::new();
    let error = refused(&json!({
        "call": "apply", "fleet": fleet.fleet(), "me": fleet.p.hex(),
        "ops": fleet.encoded()[1..], "now_ms": NOW,
    }));
    assert_eq!(error["kind"], "unfounded");
}

#[test]
fn a_forged_operation_is_refused_and_a_replay_changes_nothing() {
    let mut fleet = Fleet::new();
    let replica = fleet.replica();
    // `b` signs a removal of the phone in `a`'s name.
    let body = Body::Remove(Remove {
        member: fleet.p.id(),
        also_members: Vec::new(),
        also_hosts: Vec::new(),
        reason: Reason::Compromised,
    });
    let forged = fleet
        .log
        .draft(fleet.a.id(), NOW, body)
        .sign(&fleet.b.key)
        .unwrap();
    let after = ok(&json!({
        "call": "apply", "fleet": fleet.fleet(), "me": fleet.p.hex(),
        "log": log_of(&replica), "ops": sync::encode_ops([&forged]), "now_ms": NOW,
    }));
    assert_eq!(after["refused"], 1);
    assert_eq!(after["status"]["join"], "member");
    // Everything again, and a genuine removal of `b` with it.
    fleet.remove(&fleet.a.clone(), &fleet.b.clone());
    let after = ok(&json!({
        "call": "apply", "fleet": fleet.fleet(), "me": fleet.p.hex(),
        "log": log_of(&after), "ops": fleet.encoded(), "now_ms": NOW,
    }));
    assert_eq!(after["applied"], 1);
    assert_eq!(after["refused"], 0);
    let members = after["status"]["view"]["members"].as_array().unwrap();
    assert!(
        !members
            .iter()
            .any(|member| member["id"] == fleet.b.hex().as_str())
    );
}

#[test]
fn a_removed_phone_is_told_so_and_signs_on_nothing() {
    let mut fleet = Fleet::new();
    fleet.remove(&fleet.a.clone(), &fleet.p.clone());
    let replica = fleet.replica();
    let status = &replica["status"];
    assert_eq!(status["removed"], true);
    assert!(status.get("join").is_none());
    assert!(status.get("next").is_none());
}

#[test]
fn a_saved_log_changed_on_disk_is_refused() {
    let fleet = Fleet::new();
    let replica = fleet.replica();
    let mut bytes = sync::bytes(&log_of(&replica)).unwrap();
    // The last byte is the last operation's signature: the host's addition.
    let last = bytes.len() - 1;
    bytes[last] ^= 1;
    // Refused whole: the app then starts again from the fleet's founding
    // operation, from any computer, and checks everything again.
    let error = refused(&json!({
        "call": "apply", "fleet": fleet.fleet(), "me": fleet.p.hex(),
        "log": sync::text(&bytes), "now_ms": NOW,
    }));
    assert_eq!(error["rule"], "signature");
    // A file that is not a log at all is refused.
    let error = refused(&json!({
        "call": "apply", "fleet": fleet.fleet(), "me": fleet.p.hex(),
        "log": sync::text(b"not a log"), "now_ms": NOW,
    }));
    assert_eq!(error["rule"], "encoding");
}

#[test]
fn an_operation_whose_parents_are_missing_waits_and_is_not_kept() {
    let mut fleet = Fleet::new();
    let replica = fleet.replica();
    let first = fleet.remove(&fleet.a.clone(), &fleet.b.clone());
    let body = Body::HostRemove(sheprd_remote_control::control::fleet::HostId([7; 32]));
    let a = fleet.a.clone();
    let second = act(&mut fleet.log, &a, body);
    let after = ok(&json!({
        "call": "apply", "fleet": fleet.fleet(), "me": fleet.p.hex(),
        "log": log_of(&replica), "ops": sync::encode_ops([&second]), "now_ms": NOW,
    }));
    assert_eq!(after["pending"], 1);
    // Saved without it: the next sync brings both.
    let after = ok(&json!({
        "call": "apply", "fleet": fleet.fleet(), "me": fleet.p.hex(),
        "log": log_of(&after), "ops": sync::encode_ops([&first, &second]), "now_ms": NOW,
    }));
    assert_eq!(after["applied"], 2);
}

#[test]
fn calls_are_bounded_and_strict() {
    let fleet = Fleet::new();
    let many = vec![fleet.encoded()[0].clone(); 4097];
    let error = refused(&json!({
        "call": "apply", "fleet": fleet.fleet(), "me": fleet.p.hex(),
        "ops": many, "now_ms": NOW,
    }));
    assert_eq!(error["kind"], "bounds");
    let error = refused(&json!({
        "call": "apply", "fleet": "zz", "me": fleet.p.hex(), "ops": [], "now_ms": NOW,
    }));
    assert_eq!(error["kind"], "log");
    let error = refused(&json!({ "call": "unknown" }));
    assert_eq!(error["kind"], "request");
    let replica = fleet.replica();
    let error = refused(&json!({
        "call": "missing", "fleet": fleet.fleet(), "log": log_of(&replica),
        "heads": vec!["00".repeat(32); 1025], "now_ms": NOW,
    }));
    assert_eq!(error["kind"], "bounds");
}

#[test]
fn a_damaged_copy_gives_back_only_this_phones_own_operations() {
    let fleet = Fleet::new();
    let replica = fleet.replica();
    let log = log_of(&replica);
    let removal = json!({ "body": "remove", "member": fleet.b.hex(), "reason": "lost" });
    let drafted = ok(&json!({
        "call": "draft", "fleet": fleet.fleet(), "me": fleet.p.hex(), "log": log,
        "issued_ms": NOW, "now_ms": NOW, "bodies": [removal],
    }));
    let signed = sign_drafts(&drafted, &fleet.p.key);
    let sealed = ok(&json!({
        "call": "seal", "fleet": fleet.fleet(), "me": fleet.p.hex(), "log": log,
        "ops": signed, "now_ms": NOW,
    }));
    let saved = sync::bytes(&log_of(&sealed)).unwrap();
    let damage = |op: &[u8]| {
        let mut bytes = saved.clone();
        let at = bytes
            .windows(op.len())
            .position(|window| window == op)
            .unwrap();
        bytes[at + op.len() - 1] ^= 1;
        sync::text(&bytes)
    };
    let salvage = |log: &str| -> Vec<String> {
        let answer = ok(&json!({ "call": "salvage", "me": fleet.p.hex(), "log": log }));
        serde_json::from_value(answer["ops"].clone()).unwrap()
    };
    let start = |ops: Vec<String>| {
        let ops = [fleet.encoded(), ops].concat();
        ok(&json!({
            "call": "apply", "fleet": fleet.fleet(), "me": fleet.p.hex(),
            "ops": ops, "now_ms": NOW,
        }))
    };
    // The founding operation's signature is damaged: nothing opens the file,
    // but the phone's own removal comes back and applies to a new copy.
    let damaged = damage(&fleet.ops[0].encode());
    let error = refused(&json!({
        "call": "apply", "fleet": fleet.fleet(), "me": fleet.p.hex(), "log": damaged, "now_ms": NOW,
    }));
    assert_eq!(error["kind"], "log");
    assert_eq!(salvage(&damaged), signed);
    let again = start(salvage(&damaged));
    assert_eq!(again["refused"], 0);
    assert_eq!(again["status"]["next"]["seq"], 2);
    // Its own operation damaged: given back, but refused rather than trusted.
    let own = sync::bytes(&signed[0]).unwrap();
    let damaged = damage(&own);
    assert_eq!(start(salvage(&damaged))["refused"], 1);
}

#[test]
fn a_request_with_an_unknown_field_is_refused() {
    let fleet = Fleet::new();
    let error = refused(&json!({
        "call": "apply", "fleet": fleet.fleet(), "me": fleet.p.hex(), "ops": [],
        "now_ms": NOW, "trust": true,
    }));
    assert_eq!(error["kind"], "request");
}

#[test]
fn a_saved_copy_missing_any_operation_on_reading_is_damaged() {
    let fleet = Fleet::new();
    let replica = fleet.replica();
    let mut bytes = sync::bytes(&log_of(&replica)).unwrap();
    // The host's addition, not the founding operation: a daemon reading
    // this file drops it and goes on; the phone refuses the copy.
    let host_add = fleet.ops.last().unwrap().encode();
    let at = bytes
        .windows(host_add.len())
        .position(|window| window == host_add.as_slice())
        .unwrap();
    bytes[at + host_add.len() - 1] ^= 1;
    assert!(sheprd_remote_control::control::fleet::saved::decode(&bytes, NOW).is_ok());
    let error = refused(&json!({
        "call": "apply", "fleet": fleet.fleet(), "me": fleet.p.hex(),
        "log": sync::text(&bytes), "now_ms": NOW,
    }));
    assert_eq!(error["kind"], "damaged");
}

#[test]
fn the_phone_names_shared_history_beside_heads_a_computer_lacks() {
    let fleet = Fleet::new();
    let replica = fleet.replica();
    let drafted = ok(&json!({
        "call": "draft", "fleet": fleet.fleet(), "me": fleet.p.hex(), "log": log_of(&replica),
        "issued_ms": NOW, "now_ms": NOW,
        "bodies": [{ "body": "host_remove", "host": fleet.host }],
    }));
    let signed = sign_drafts(&drafted, &fleet.p.key);
    let sealed = ok(&json!({
        "call": "seal", "fleet": fleet.fleet(), "me": fleet.p.hex(), "log": log_of(&replica),
        "ops": signed, "now_ms": NOW,
    }));
    let known: Vec<String> = serde_json::from_value(sealed["known"].clone()).unwrap();
    // Its own new head first, then the history the computer shares.
    assert_eq!(known[0], drafted["drafts"][0]["id"]);
    let shared: Vec<String> = fleet.log.heads().iter().map(ToString::to_string).collect();
    assert!(shared.iter().all(|head| known.contains(head)));
    // A computer that lacks the phone's operation credits the rest: it
    // would send nothing it shares.
    let heads: Vec<sheprd_remote_control::control::fleet::OpId> = known
        .iter()
        .filter_map(|id| sheprd_remote_control::control::fleet::OpId::from_hex(id))
        .collect();
    assert!(fleet.log.missing_for(&heads).is_empty());
}
