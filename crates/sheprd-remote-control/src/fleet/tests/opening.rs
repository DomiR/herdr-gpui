//! Only a member's own host reads a fleet session's opening: a shared
//! host that is no member gets SSH from its first byte.

use super::*;
use crate::control::fleet::HostKind;
use crate::desktop::record::Record;

/// The record `node` keeps for `route`.
fn kept(node: &Node, route: &str) -> Record {
    let probe = Record {
        computer: String::new(),
        identity: String::new(),
        relays: Vec::new(),
        route: route.to_owned(),
        fleet: true,
        host: None,
        opening: false,
    };
    Record::load(node.dir.path(), &probe.id().unwrap()).unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn only_a_members_host_is_sent_the_opening() {
    let (a, b, c) = idle().await;
    // c shares a Sheprd host that is no member.
    let other = Node::new("127.0.0.1:9".parse().unwrap(), 9).await;
    let mut intro = me::intro(&other.state, now_ms()).unwrap();
    intro.kind = HostKind::Sheprd;
    let route = intro.record.route.clone();
    c.fleet.sign(Body::HostAdd(intro), now_ms()).unwrap();
    b.fleet.receive(c.ops(), now_ms()).unwrap();
    grants::apply(&b.state, &b.fleet).unwrap();
    let shared = kept(&b, &route);
    assert!(shared.fleet);
    assert!(!shared.opening);
    let member = kept(&b, &a.relays.info().unwrap().route);
    assert!(member.fleet);
    assert!(member.opening);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_shared_record_without_the_opening_starts_with_ssh() {
    let (http, _, _) = relay().await;
    let (mut a, b) = (Node::new(http, 1).await, Node::new(http, 2).await);
    // a is no member of anything: b is paired with it one by one, and b's
    // record for it, the fleet's, says a reads no opening.
    proof::pair(&a, b.dir.path());
    a.start().await;
    let info = a.relays.info().unwrap();
    let record = Record {
        computer: String::new(),
        identity: info.identity,
        relays: info.relays,
        route: info.route,
        fleet: true,
        host: Some("11".repeat(32)),
        opening: false,
    };
    let id = record.save(b.dir.path(), &[]).unwrap();
    let host = sheprd_client::relay_host::host_name(&id);
    let mut opened = None;
    for _ in 0..100 {
        if let Ok(tunnel) = crate::desktop::connect::open(b.dir.path(), &host).await {
            opened = Some(tunnel);
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    let (_connection, mut tunnel) = opened.expect("never let through");
    let mut greeting = [0; 14];
    tunnel.io.read_exact(&mut greeting).await.unwrap();
    // The SSH server echoes: the first bytes back are ours, no opening.
    tunnel.io.write_all(b"ping").await.unwrap();
    let mut echoed = [0; 4];
    tunnel.io.read_exact(&mut echoed).await.unwrap();
    assert_eq!(&echoed, b"ping");
}
