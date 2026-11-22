//! A removal its history proves invalid ends the suspension it caused,
//! whether that history arrives while it waits or after a restart; one
//! the history confirms still removes its target.

use super::*;
use crate::control::fleet::{HostKind, Reason, Remove, Unsigned};

/// a, b, and c in one fleet, and an operation a signed that b lacks.
async fn ahead() -> (Node, Node, Node, Op) {
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
    let other = Node::new("127.0.0.1:9".parse().unwrap(), 9).await;
    let mut intro = me::intro(&other.state, now_ms()).unwrap();
    intro.kind = HostKind::Sheprd;
    let missing = a.fleet.sign(Body::HostAdd(intro), now_ms()).unwrap();
    (a, b, c, missing)
}

/// a's removal of `target`, on `missing`, with `missing`'s own sequence
/// number when `reused`: the history then refuses it.
fn removal(a: &Node, target: &Node, missing: &Op, reused: bool) -> Op {
    let inner = a.fleet.inner();
    let mut unsigned: Unsigned = inner.log.as_ref().unwrap().draft(
        a.fleet.me(),
        now_ms(),
        Body::Remove(Remove {
            member: target.fleet.me(),
            also_members: Vec::new(),
            also_hosts: Vec::new(),
            reason: Reason::Compromised,
        }),
    );
    assert_eq!(unsigned.parents, vec![missing.id()]);
    if reused {
        unsigned.seq = missing.unsigned().seq;
    }
    unsigned.sign(a.fleet.key()).unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn a_refused_removal_lifts_its_suspension() {
    let (a, b, c, missing) = ahead().await;
    let invalid = removal(&a, &c, &missing, true);
    b.fleet.receive(vec![invalid], now_ms()).unwrap();
    assert_eq!(b.fleet.suspended(), vec![c.fleet.me()]);
    let changed = b.fleet.watch();
    b.fleet.receive(vec![missing], now_ms()).unwrap();
    assert!(b.fleet.suspended().is_empty());
    assert!(b.fleet.inner().unconfirmed.is_empty());
    assert!(has(&b, &c));
    // Grants are rewritten, and the file forgets the suspension.
    assert!(changed.has_changed().unwrap());
    let again = Fleet::load(b.dir.path(), now_ms()).unwrap();
    assert!(again.inner().unconfirmed.is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_refused_removal_lifts_its_suspension_after_a_restart() {
    let (a, b, c, missing) = ahead().await;
    let invalid = removal(&a, &c, &missing, true);
    b.fleet.receive(vec![invalid], now_ms()).unwrap();
    let again = Fleet::load(b.dir.path(), now_ms()).unwrap();
    assert_eq!(again.suspended(), vec![c.fleet.me()]);
    again.receive(vec![missing], now_ms()).unwrap();
    assert!(again.suspended().is_empty());
    assert!(again.inner().unconfirmed.is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_valid_removal_kept_across_a_restart_still_removes() {
    let (a, b, c, missing) = ahead().await;
    let valid = removal(&a, &c, &missing, false);
    b.fleet.receive(vec![valid], now_ms()).unwrap();
    let again = Fleet::load(b.dir.path(), now_ms()).unwrap();
    assert_eq!(again.suspended(), vec![c.fleet.me()]);
    // The history alone, without the removal: the removal kept with the
    // suspension is checked again, and takes effect.
    again.receive(vec![missing], now_ms()).unwrap();
    let inner = again.inner();
    assert!(!inner.log.as_ref().unwrap().is_member(&c.fleet.me()));
    assert!(inner.unconfirmed.is_empty());
}

fn has(node: &Node, member: &Node) -> bool {
    node.view()
        .members
        .iter()
        .any(|entry| entry.id == member.me())
}
