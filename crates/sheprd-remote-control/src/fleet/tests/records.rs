//! The relay records the fleet writes: it never deletes one pairing took
//! over, it follows a host's signed new tunnel identity, and no host takes
//! over another's record by claiming its route.

use super::*;
use crate::desktop::record::Record;
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use std::collections::BTreeSet;

fn record(identity: &str, fleet: bool) -> Record {
    hosted(identity, fleet, fleet.then_some("a"))
}

/// A record written for the fleet host `host`.
fn hosted(identity: &str, fleet: bool, host: Option<&str>) -> Record {
    Record {
        computer: "Studio".into(),
        identity: identity.into(),
        relays: vec!["https://relay.example.org".into()],
        route: URL_SAFE_NO_PAD.encode([7; 32]),
        fleet,
        host: host.map(str::to_owned),
        opening: false,
    }
}

fn keys() -> Vec<String> {
    vec!["ssh-ed25519 AAAA".to_owned()]
}

#[test]
fn a_host_paired_after_the_fleet_wrote_it_is_kept() {
    let dir = tempfile::tempdir().unwrap();
    let fleet = record("identity-one", true);
    let id = fleet.id().unwrap();
    grants::write_records(dir.path(), &[(fleet, keys())], &grants::Routes::default()).unwrap();
    // Paired here over Bluetooth since: pairing owns the record now.
    let paired = record("identity-one", false);
    paired.save(dir.path(), &keys()).unwrap();
    // Granted still, then no longer: neither deletes it.
    grants::write_records(
        dir.path(),
        &[(record("identity-one", true), keys())],
        &grants::Routes::default(),
    )
    .unwrap();
    assert_eq!(Record::load(dir.path(), &id).unwrap(), paired);
    grants::write_records(dir.path(), &[], &grants::Routes::default()).unwrap();
    assert_eq!(Record::load(dir.path(), &id).unwrap(), paired);
}

#[test]
fn a_route_whose_owner_is_unclear_keeps_its_last_record() {
    let dir = tempfile::tempdir().unwrap();
    let kept = record("identity-one", true);
    let id = kept.id().unwrap();
    grants::write_records(
        dir.path(),
        &[(kept.clone(), keys())],
        &grants::Routes::default(),
    )
    .unwrap();
    // Two hosts name the route: neither gets it, and the record stays.
    let unclear = grants::Routes {
        unclear: BTreeSet::from([kept.route.clone()]),
        ..Default::default()
    };
    grants::write_records(dir.path(), &[], &unclear).unwrap();
    assert_eq!(Record::load(dir.path(), &id).unwrap(), kept);
    // Once the route is clear again and no host is granted, it goes.
    grants::write_records(dir.path(), &[], &grants::Routes::default()).unwrap();
    assert!(Record::load(dir.path(), &id).is_err());
}

#[test]
fn a_fleet_host_that_changed_its_tunnel_identity_keeps_its_record() {
    let dir = tempfile::tempdir().unwrap();
    let id = record("identity-one", true).id().unwrap();
    grants::write_records(
        dir.path(),
        &[(record("identity-one", true), keys())],
        &grants::Routes::default(),
    )
    .unwrap();
    let moved = record("identity-two", true);
    grants::write_records(
        dir.path(),
        &[(moved.clone(), keys())],
        &grants::Routes::default(),
    )
    .unwrap();
    assert_eq!(Record::load(dir.path(), &id).unwrap(), moved);
}

#[test]
fn pairing_still_cannot_replace_an_identity() {
    let dir = tempfile::tempdir().unwrap();
    let id = record("identity-one", true).id().unwrap();
    grants::write_records(
        dir.path(),
        &[(record("identity-one", true), keys())],
        &grants::Routes::default(),
    )
    .unwrap();
    assert!(matches!(
        record("identity-two", false).save(dir.path(), &keys()),
        Err(crate::desktop::Error::RecordConflict(_))
    ));
    // Nor can the fleet take over another identity pairing wrote.
    let dir = tempfile::tempdir().unwrap();
    record("identity-one", false)
        .save(dir.path(), &keys())
        .unwrap();
    assert!(matches!(
        record("identity-two", true).save(dir.path(), &keys()),
        Err(crate::desktop::Error::RecordConflict(_))
    ));
    assert!(!Record::load(dir.path(), &id).unwrap().fleet);
}

#[test]
fn another_fleet_host_cannot_replace_a_records_identity() {
    let dir = tempfile::tempdir().unwrap();
    let kept = hosted("identity-one", true, Some("a"));
    let id = kept.save(dir.path(), &keys()).unwrap();
    for other in [
        hosted("identity-two", true, Some("b")),
        hosted("identity-one", true, Some("b")),
        hosted("identity-two", true, None),
    ] {
        assert!(
            matches!(
                other.save(dir.path(), &keys()),
                Err(crate::desktop::Error::RecordConflict(_))
            ),
            "{other:?}"
        );
    }
    assert_eq!(Record::load(dir.path(), &id).unwrap(), kept);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_host_claiming_another_hosts_route_changes_nothing() {
    let (a, b, c) = idle().await;
    let route = a.relays.info().unwrap().route;
    // c signs, with its own authority, a record naming a's route.
    let (mut claim, _) = me::record(&c.state, now_ms() + 1).unwrap();
    claim.route = route.clone();
    let signature = c
        .state
        .authority
        .as_ref()
        .unwrap()
        .sign(&claim.message().unwrap())
        .unwrap();
    let own = |node: &Node| {
        let a_record = Record {
            computer: String::new(),
            identity: String::new(),
            relays: Vec::new(),
            route: route.clone(),
            fleet: true,
            host: None,
            opening: false,
        };
        Record::load(node.dir.path(), &a_record.id().unwrap()).unwrap()
    };
    grants::apply(&b.state, &b.fleet).unwrap();
    let before = own(&b);
    assert_eq!(before.identity, a.relays.info().unwrap().identity);
    c.fleet
        .sign(
            Body::HostSeen(crate::control::fleet::HostSeen {
                record: claim,
                signature: Some(signature),
            }),
            now_ms(),
        )
        .unwrap();
    b.fleet.receive(c.ops(), now_ms()).unwrap();
    // The log does not count c's claim, and the records keep a's route a's.
    let claimants = {
        let inner = b.fleet.inner();
        let state = inner.log.as_ref().unwrap().state();
        state
            .hosts
            .values()
            .filter(|host| host.record.route == route)
            .count()
    };
    assert_eq!(claimants, 1);
    grants::apply(&b.state, &b.fleet).unwrap();
    assert_eq!(own(&b), before);
}

#[test]
fn a_failed_refresh_keeps_a_granted_hosts_record() {
    let dir = tempfile::tempdir().unwrap();
    let kept = record("identity-one", true);
    let id = kept.id().unwrap();
    grants::write_records(
        dir.path(),
        &[(kept.clone(), keys())],
        &grants::Routes::default(),
    )
    .unwrap();
    // Its known hosts cannot be written: a directory stands there.
    let known = sheprd_client::relay_host::known_hosts(
        dir.path(),
        &sheprd_client::relay_host::host_name(&id),
    );
    std::fs::remove_file(&known).unwrap();
    std::fs::create_dir(&known).unwrap();
    std::fs::write(known.join("blocker"), b"").unwrap();
    let moved = record("identity-two", true);
    assert!(
        grants::write_records(
            dir.path(),
            &[(moved.clone(), keys())],
            &grants::Routes::default()
        )
        .is_err()
    );
    // Still granted, so not cleaned up; the error lets the write be retried.
    assert!(Record::load(dir.path(), &id).is_ok());
    std::fs::remove_dir_all(&known).unwrap();
    grants::write_records(
        dir.path(),
        &[(moved.clone(), keys())],
        &grants::Routes::default(),
    )
    .unwrap();
    assert_eq!(Record::load(dir.path(), &id).unwrap(), moved);
}
