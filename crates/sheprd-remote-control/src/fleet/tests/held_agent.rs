//! A plain host two members added with keys that disagree is held: its
//! agent lets no member reach SSH until a person picks, yet removals
//! still reach it, from the access file and from a member's daemon.

use super::agent::{access_file, fleet_apply, plain_host};
use super::*;
use crate::control::fleet::{Reason, sync};
use sheprd_relay_agent::{access::Id, fleet::Held};

#[tokio::test(flavor = "multi_thread")]
async fn a_held_hosts_agent_shuts_ssh_and_still_takes_removals() {
    let (http, _, _) = relay().await;
    let (mut a, b, c) = (
        Node::new(http, 1).await,
        Node::new(http, 2).await,
        Node::new(http, 3).await,
    );
    a.ask(Ask::Found);
    join(&a, &b);
    join(&a, &c);
    let (dir, agent, _stop, intro) = plain_host(http).await;
    let host = intro.id().unwrap();
    // a shares the host, which takes its file and lets members in.
    a.ask(Ask::AddHost {
        intro: sync::text(&intro.to_bytes().unwrap()),
        deny: Vec::new(),
    });
    fleet_apply(&a, &host, dir.path(), &agent);
    let c_id = Id(c.fleet.me().0);
    assert!(Held::load(dir.path()).unwrap().reaches_ssh(&c_id));

    // b, not having heard, adds it with another key list: held.
    let mut other = intro.clone();
    other.record.ssh_host_keys.push(format!(
        "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAI{}",
        "Q".repeat(43)
    ));
    b.ask(Ask::AddHost {
        intro: sync::text(&other.to_bytes().unwrap()),
        deny: Vec::new(),
    });
    a.fleet.receive(b.ops(), now_ms()).unwrap();
    assert!(a.fleet.inner().log.as_ref().unwrap().state().hosts[&host].held);

    // The next file pauses it: no member reaches SSH, none has a line.
    let file = access_file(&a, &host, dir.path());
    assert!(file.paused);
    let held = fleet_apply(&a, &host, dir.path(), &agent);
    assert!(!held.reaches_ssh(&c_id));
    assert!(held.admits(&c_id), "members still pass removals");
    let keys = std::fs::read_to_string(dir.path().join("authorized_keys")).unwrap_or_default();
    assert!(!keys.contains("sheprd-fleet:"), "{keys}");

    // a's daemon still passes its removals to the held host's agent.
    a.start().await;
    a.ask(Ask::Remove {
        member: c.me(),
        also_members: Vec::new(),
        also_hosts: Vec::new(),
        reason: Reason::Compromised,
    });
    until(20, || {
        Held::load(dir.path())
            .unwrap()
            .suspended
            .contains_key(&c_id)
    })
    .await;
    let held = fleet_apply(&a, &host, dir.path(), &agent);
    assert!(held.access.unwrap().removed.contains(&c_id));
}
