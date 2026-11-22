//! The fixture the iOS app's `SheprdFleetLog` tests read: a fleet's
//! operations, the phone's key, what its replica says, and the bytes of a
//! draft, so the Swift side is checked against this crate as the app links
//! it. `HERDR_UPDATE_FIXTURES=1` rewrites it.

use super::*;
use std::path::PathBuf;

#[test]
fn replica_vector() {
    let fleet = Fleet::new();
    let replica = fleet.replica();
    let log = log_of(&replica);
    let removal = json!({ "body": "remove", "member": fleet.b.hex(), "reason": "lost" });
    let drafted = ok(&json!({
        "call": "draft", "fleet": fleet.fleet(), "me": fleet.p.hex(), "log": log,
        "issued_ms": NOW, "now_ms": NOW, "bodies": [removal],
    }));
    let status = &replica["status"];
    let vector = json!({
        "fleet": fleet.fleet(),
        "me": fleet.p.hex(),
        "phone_key": "03".repeat(32),
        "now_ms": NOW,
        "ops": fleet.encoded(),
        "removed_member": fleet.b.hex(),
        "host": fleet.host,
        "expect": {
            "heads": status["heads"],
            "next": status["next"],
            "members": status["view"]["members"].as_array().unwrap().len(),
            "draft": drafted["drafts"][0]["signed"],
            "draft_id": drafted["drafts"][0]["id"],
        },
    });
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../ios/SheprdRemoteCore/Tests/SheprdFleetLogTests/Fixtures/replica.json");
    let text = format!("{}\n", serde_json::to_string_pretty(&vector).unwrap());
    if std::env::var_os("HERDR_UPDATE_FIXTURES").is_some() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, text).unwrap();
        return;
    }
    let expected = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    let expected: Value = serde_json::from_str(&expected).unwrap();
    assert_eq!(
        vector,
        expected,
        "{} is out of date; run with HERDR_UPDATE_FIXTURES=1",
        path.display()
    );
}
