//! Removals passed between daemons inside the tunnel, with no Sheprd
//! running anywhere: they flood, they reach a member that was away, a
//! forged or replayed one changes nothing, and one whose history has not
//! arrived suspends its target at once.

use super::*;
use crate::control::fleet::{
    Body, HostKind, Reason, Remove,
    removal::{Message, PURPOSE_REMOVAL},
};
use crate::desktop::record::Record;

fn remove(by: &Node, member: &Node) {
    by.ask(Ask::Remove {
        member: member.me(),
        also_members: Vec::new(),
        also_hosts: Vec::new(),
        reason: Reason::Compromised,
    });
}

fn has(node: &Node, member: &Node) -> bool {
    node.view()
        .members
        .iter()
        .any(|entry| entry.id == member.me())
}

#[tokio::test(flavor = "multi_thread")]
async fn a_removal_floods_to_every_member() {
    let (a, b, c) = three().await;
    until(10, || b.lines().len() == 2).await;
    remove(&a, &c);
    // b hears of it from a's daemon, through the relay, not from Sheprd.
    until(15, || !has(&b, &c)).await;
    until(10, || b.lines().len() == 1).await;
    assert!(!b.lines().iter().any(|line| line.ends_with(&c.me()[..16])));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_member_away_during_a_removal_gets_it_when_it_is_back() {
    let (http, _, _) = relay().await;
    let (mut a, mut b, mut c) = (
        Node::new(http, 1).await,
        Node::new(http, 2).await,
        Node::new(http, 3).await,
    );
    a.ask(Ask::Found);
    join(&a, &b);
    join(&a, &c);
    b.fleet.receive(a.ops(), now_ms()).unwrap();
    a.start().await;
    c.start().await;
    // b is away: not listening anywhere.
    remove(&a, &c);
    tokio::time::sleep(Duration::from_secs(1)).await;
    assert!(has(&b, &c));
    b.start().await;
    // a's retries back off from five seconds.
    until(20, || !has(&b, &c)).await;
}

/// Opens a removal exchange from `from` to `to` with `from`'s own keys.
pub(super) async fn exchange(from: &Node, to: &Node) -> sheprd_tunnel::stream::Tunnel {
    let info = to.relays.info().unwrap();
    let record = Record {
        computer: String::new(),
        identity: info.identity,
        relays: info.relays,
        route: info.route,
        fleet: true,
        host: None,
        opening: true,
    };
    let keys = crate::desktop::keys::Keys::load_or_create(from.dir.path()).unwrap();
    // `to` names `from`'s dial key on the relay just after it applies the
    // log; until then the relay answers `offline`.
    let mut opened = None;
    for _ in 0..100 {
        if let Ok(tunnel) = crate::desktop::connect::open_record(&record, &keys).await {
            opened = Some(tunnel);
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    let (connection, mut tunnel) = opened.expect("never let through");
    // The connection lives as long as the test.
    std::mem::forget(connection);
    let mut opening = vec![PURPOSE_REMOVAL];
    opening.extend_from_slice(&crate::control::fleet::removal::prove(
        from.fleet.key(),
        &tunnel.hash,
    ));
    tunnel.io.write_all(&opening).await.unwrap();
    tunnel
}

pub(super) async fn send(tunnel: &mut sheprd_tunnel::stream::Tunnel, message: &Message) {
    let bytes = message.encode().unwrap();
    tunnel
        .io
        .write_all(&u32::try_from(bytes.len()).unwrap().to_be_bytes())
        .await
        .unwrap();
    tunnel.io.write_all(&bytes).await.unwrap();
}

pub(super) async fn receive(tunnel: &mut sheprd_tunnel::stream::Tunnel) -> Message {
    let mut length = [0; 4];
    tunnel.io.read_exact(&mut length).await.unwrap();
    let mut bytes = vec![0; usize::try_from(u32::from_be_bytes(length)).unwrap()];
    tunnel.io.read_exact(&mut bytes).await.unwrap();
    Message::decode(&bytes).unwrap()
}

/// a, b, and c in one fleet, with only b listening: a's and c's daemons
/// pass nothing on their own.
async fn quiet() -> (Node, Node, Node) {
    let (http, _, _) = relay().await;
    let (a, mut b, c) = (
        Node::new(http, 1).await,
        Node::new(http, 2).await,
        Node::new(http, 3).await,
    );
    a.ask(Ask::Found);
    join(&a, &b);
    join(&a, &c);
    for node in [&b, &c] {
        node.fleet.receive(a.ops(), now_ms()).unwrap();
    }
    b.start().await;
    (a, b, c)
}

#[tokio::test(flavor = "multi_thread")]
async fn a_forged_or_replayed_removal_changes_nothing() {
    let (a, b, c) = quiet().await;
    until(10, || b.lines().len() == 2).await;
    // c signs a removal of a with a key that is not its own.
    let forged = {
        let inner = c.fleet.inner();
        let log = inner.log.as_ref().unwrap();
        log.draft(
            c.fleet.me(),
            now_ms(),
            Body::Remove(Remove {
                member: a.fleet.me(),
                also_members: Vec::new(),
                also_hosts: Vec::new(),
                reason: Reason::Lost,
            }),
        )
        .sign(&SigningKey::from_slice(&[0x42; 32]).unwrap())
        .unwrap()
    };
    let mut tunnel = exchange(&c, &b).await;
    send(&mut tunnel, &Message::Removal(vec![forged.encode()])).await;
    assert_eq!(receive(&mut tunnel).await, Message::Held(Vec::new()));
    assert!(has(&b, &a));
    // A real removal, then the same one again.
    remove(&a, &c);
    let removal = a
        .ops()
        .into_iter()
        .find(|op| matches!(op.body(), Body::Remove(_)))
        .unwrap();
    for _ in 0..2 {
        let mut tunnel = exchange(&a, &b).await;
        send(&mut tunnel, &Message::Removal(vec![removal.encode()])).await;
        assert_eq!(
            receive(&mut tunnel).await,
            Message::Held(vec![removal.id()])
        );
    }
    assert!(!has(&b, &c));
    assert_eq!(b.view().members.len(), 2);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_removal_without_its_history_suspends_first() {
    let (a, b, c) = quiet().await;
    until(10, || b.lines().len() == 2).await;
    // a adds a host b has not heard of, then removes c on top of it.
    let (intro, _) = {
        let other = Node::new("127.0.0.1:9".parse().unwrap(), 9).await;
        (me::intro(&other.state, now_ms()).unwrap(), other)
    };
    let mut intro = intro;
    intro.kind = HostKind::Sheprd;
    a.fleet.sign(Body::HostAdd(intro), now_ms()).unwrap();
    remove(&a, &c);
    let ops = a.ops();
    let removal = ops
        .iter()
        .find(|op| matches!(op.body(), Body::Remove(_)))
        .unwrap();
    let mut tunnel = exchange(&a, &b).await;
    send(&mut tunnel, &Message::Removal(vec![removal.encode()])).await;
    let Message::Want(wanted) = receive(&mut tunnel).await else {
        panic!("b did not ask for the history");
    };
    // Before the history arrives, c is already cut off on b.
    assert_eq!(b.fleet.suspended(), vec![c.fleet.me()]);
    until(10, || b.lines().len() == 1).await;
    let history: Vec<Vec<u8>> = wanted
        .iter()
        .map(|id| ops.iter().find(|op| op.id() == *id).unwrap().encode())
        .collect();
    send(&mut tunnel, &Message::Removal(history)).await;
    assert_eq!(
        receive(&mut tunnel).await,
        Message::Held(vec![removal.id()])
    );
    assert!(!has(&b, &c));
    assert!(b.fleet.suspended().is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_suspension_ends_with_its_signer_and_never_cuts_off_this_computer() {
    let (a, b, c) = quiet().await;
    until(10, || b.lines().len() == 2).await;
    let (intro, _) = {
        let other = Node::new("127.0.0.1:9".parse().unwrap(), 9).await;
        (me::intro(&other.state, now_ms()).unwrap(), other)
    };
    let mut intro = intro;
    intro.kind = HostKind::Sheprd;
    a.fleet.sign(Body::HostAdd(intro), now_ms()).unwrap();
    // a removes c, and then b, each on the host b has not heard of.
    remove(&a, &c);
    remove(&a, &b);
    let removals: Vec<Op> = a
        .ops()
        .into_iter()
        .filter(|op| matches!(op.body(), Body::Remove(_)))
        .collect();
    b.fleet.receive(removals, now_ms()).unwrap();
    // c is cut off at once; b never cuts itself off on a removal it
    // cannot check yet.
    assert_eq!(b.fleet.suspended(), vec![c.fleet.me()]);
    // Once a is no longer a member, what it signed suspends nothing.
    b.ask(Ask::Remove {
        member: a.me(),
        also_members: Vec::new(),
        also_hosts: Vec::new(),
        reason: Reason::Compromised,
    });
    assert!(b.fleet.suspended().is_empty());
    until(10, || {
        b.lines().iter().any(|line| line.ends_with(&c.me()[..16]))
    })
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_suspended_member_cannot_lift_its_own_suspension() {
    let (a, b, c) = quiet().await;
    until(10, || b.lines().len() == 2).await;
    let (intro, _) = {
        let other = Node::new("127.0.0.1:9".parse().unwrap(), 9).await;
        (me::intro(&other.state, now_ms()).unwrap(), other)
    };
    let mut intro = intro;
    intro.kind = HostKind::Sheprd;
    a.fleet.sign(Body::HostAdd(intro), now_ms()).unwrap();
    remove(&a, &c);
    let removal = a
        .ops()
        .into_iter()
        .find(|op| matches!(op.body(), Body::Remove(_)))
        .unwrap();
    b.fleet.receive(vec![removal], now_ms()).unwrap();
    assert_eq!(b.fleet.suspended(), vec![c.fleet.me()]);
    // c, suspended on b, removes a, whose removal suspends it.
    let counter = {
        let inner = c.fleet.inner();
        inner
            .log
            .as_ref()
            .unwrap()
            .draft(c.fleet.me(), now_ms(), remove_body_of(&a))
            .sign(c.fleet.key())
            .unwrap()
    };
    b.fleet.receive(vec![counter], now_ms()).unwrap();
    assert!(!has(&b, &a));
    assert_eq!(b.fleet.suspended(), vec![c.fleet.me()]);
}

fn remove_body_of(member: &Node) -> Body {
    Body::Remove(Remove {
        member: member.fleet.me(),
        also_members: Vec::new(),
        also_hosts: Vec::new(),
        reason: Reason::Compromised,
    })
}
