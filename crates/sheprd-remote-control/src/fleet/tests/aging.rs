//! On an idle daemon, a suspension whose removal's history never comes
//! ends by the clock alone, and a failed share never reaches the disk.

use super::*;
use crate::control::fleet::{HostKind, Reason, Remove};

#[tokio::test(flavor = "multi_thread")]
async fn a_suspension_ends_on_its_own_time() {
    let (http, _, _) = relay().await;
    let (a, b, c) = (
        Node::new(http, 1).await,
        Node::new(http, 2).await,
        Node::new(http, 3).await,
    );
    a.ask(Ask::Found);
    join(&a, &b);
    join(&a, &c);
    b.fleet.receive(a.ops(), now_ms()).unwrap();
    // a removes c on a host b has not heard of.
    let other = Node::new("127.0.0.1:9".parse().unwrap(), 9).await;
    let mut intro = me::intro(&other.state, now_ms()).unwrap();
    intro.kind = HostKind::Sheprd;
    a.fleet.sign(Body::HostAdd(intro), now_ms()).unwrap();
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
    let changed = b.fleet.watch();
    // A day on: still suspended, nothing announced.
    b.fleet.tick(now_ms() + 24 * 60 * 60 * 1000).unwrap();
    assert_eq!(b.fleet.suspended(), vec![c.fleet.me()]);
    // Past the limit, with no other change: it ends, and the grants are
    // rewritten.
    b.fleet.tick(now_ms() + UNCONFIRMED_TTL_MS + 1).unwrap();
    assert!(b.fleet.suspended().is_empty());
    assert!(changed.has_changed().unwrap());
    assert!(b.fleet.inner().unconfirmed.is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_share_whose_save_fails_is_not_on_disk() {
    let (http, _, _) = relay().await;
    let a = Node::new(http, 1).await;
    assert_eq!(a.ask(Ask::Found), Answer::Done);
    let heads = a.fleet.inner().log.as_ref().unwrap().heads().to_vec();
    // The peers file cannot be written: a directory stands there.
    let peers = a.dir.path().join("fleet/peers.json");
    let _ = std::fs::remove_file(&peers);
    std::fs::create_dir(&peers).unwrap();
    std::fs::write(peers.join("blocker"), b"").unwrap();
    let other = Node::new("127.0.0.1:9".parse().unwrap(), 9).await;
    let mut intro = me::intro(&other.state, now_ms()).unwrap();
    intro.kind = HostKind::Sheprd;
    let result = ask::answer(
        &a.state,
        &a.fleet,
        Ask::AddHost {
            intro: sync::text(&intro.to_bytes().unwrap()),
            deny: Vec::new(),
        },
        now_ms(),
    );
    assert!(result.is_err());
    assert_eq!(
        a.fleet.inner().log.as_ref().unwrap().heads(),
        heads.as_slice()
    );
    // The log on disk is the one held: a restart brings no share back.
    std::fs::remove_dir_all(&peers).unwrap();
    let again = Fleet::load(a.dir.path(), now_ms()).unwrap();
    assert_eq!(
        again.inner().log.as_ref().unwrap().heads(),
        heads.as_slice()
    );
}
