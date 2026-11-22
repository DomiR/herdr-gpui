//! A suspension stays on disk until the log that settles it is written:
//! a save cut short between the two files never lets a removed member
//! back in after a restart.

use super::*;
use crate::control::fleet::{HostKind, Reason, Remove};

#[tokio::test(flavor = "multi_thread")]
async fn a_settled_suspension_outlives_a_log_that_was_not_written() {
    let (a, b, c) = idle().await;
    // a removes c on a host b has not heard of.
    let other = Node::new("127.0.0.1:9".parse().unwrap(), 9).await;
    let mut intro = me::intro(&other.state, now_ms()).unwrap();
    intro.kind = HostKind::Sheprd;
    let history = a.fleet.sign(Body::HostAdd(intro), now_ms()).unwrap();
    let removal = a
        .fleet
        .sign(
            Body::Remove(Remove {
                member: c.fleet.me(),
                also_members: Vec::new(),
                also_hosts: Vec::new(),
                reason: Reason::Compromised,
            }),
            now_ms(),
        )
        .unwrap();
    b.fleet.receive(vec![removal], now_ms()).unwrap();
    assert_eq!(b.fleet.suspended(), vec![c.fleet.me()]);
    // The history arrives, settling the suspension, but the log cannot be
    // written: a directory stands where it goes.
    let log = b.dir.path().join("fleet/log.bin");
    let old = std::fs::read(&log).unwrap();
    std::fs::remove_file(&log).unwrap();
    std::fs::create_dir(&log).unwrap();
    std::fs::write(log.join("blocker"), b"").unwrap();
    assert!(b.fleet.receive(vec![history.clone()], now_ms()).is_err());
    assert!(b.fleet.inner().unconfirmed.is_empty());
    // A restart finds the old log, and the suspension beside it.
    std::fs::remove_dir_all(&log).unwrap();
    std::fs::write(&log, &old).unwrap();
    let again = Fleet::load(b.dir.path(), now_ms()).unwrap();
    assert!(again.inner().log.as_ref().unwrap().is_member(&c.fleet.me()));
    assert_eq!(again.suspended(), vec![c.fleet.me()]);
    // Once a save writes the settling log, the suspension leaves the disk.
    again.receive(vec![history], now_ms()).unwrap();
    drop(again);
    let settled = Fleet::load(b.dir.path(), now_ms()).unwrap();
    assert!(
        !settled
            .inner()
            .log
            .as_ref()
            .unwrap()
            .is_member(&c.fleet.me())
    );
    let peers = std::fs::read_to_string(b.dir.path().join("fleet/peers.json")).unwrap();
    assert!(!peers.contains(&removal_id(&settled, &c)));
}

/// The id, hex, of the removal of `member` that `fleet` holds.
fn removal_id(fleet: &Fleet, member: &Node) -> String {
    let inner = fleet.inner();
    inner
        .log
        .as_ref()
        .unwrap()
        .ops()
        .find(|op| match op.body() {
            Body::Remove(remove) => remove.member == member.fleet.me(),
            _ => false,
        })
        .unwrap()
        .id()
        .to_string()
}
