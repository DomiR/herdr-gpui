//! What a sync finds is kept on the machine's link, the host it serves
//! included, so a plain host removed and then shared again is synced again.

use super::*;
use sheprd_remote_control::control::fleet::{HostKind, HostView};

fn plain_host(id: &str) -> HostView {
    HostView {
        id: id.into(),
        name: "build-01".into(),
        kind: HostKind::Agent,
        user: "deploy".into(),
        added_by: "a".into(),
        grants: Vec::new(),
        relays: Vec::new(),
        target: None,
        rev: 1,
        held: false,
        flagged: false,
    }
}

#[test]
fn a_removed_plain_host_shared_again_is_synced_again() {
    let mut view = fleet_view(&["h"]);
    let mut fleet = Fleet {
        view: Some(view.clone()),
        reads: 1,
        ..Fleet::default()
    };
    // The plain-host sync told its agent to forget the fleet, and named
    // the host its key proves.
    let mut machine = Link::default();
    machine.settle(
        Standing::Removed,
        Vec::new(),
        vec!["h".into()],
        Some("plain-1".into()),
    );
    fleet.links.insert("m1".into(), machine);
    fleet.answered(1, Ok(Some(view.clone())));
    assert_eq!(fleet.links["m1"].standing, Standing::Removed);
    // Another computer shares it again.
    view.hosts.push(plain_host("plain-1"));
    fleet.reads = 2;
    fleet.answered(2, Ok(Some(view)));
    assert_eq!(fleet.links["m1"].standing, Standing::Unknown);
}

#[test]
fn a_sync_that_cannot_tell_the_host_keeps_the_one_it_knew() {
    let mut machine = Link::default();
    machine.settle(
        Standing::Agent,
        Vec::new(),
        Vec::new(),
        Some("plain-1".into()),
    );
    machine.settle(Standing::NoDaemon, Vec::new(), Vec::new(), None);
    assert_eq!(machine.host.as_deref(), Some("plain-1"));
    assert_eq!(machine.standing, Standing::NoDaemon);
    assert!(!machine.running);
}
