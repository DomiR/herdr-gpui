//! A removal a plain host's agent holds is owed to it again once the log
//! learns it takes a member the agent was not told of.

use super::agent::plain_host;
use super::*;
use crate::control::fleet::{HostId, MemberId, OpId, Reason, sync};

/// Removals owed, and the members beyond those they name.
type Owed = (Vec<OpId>, Vec<(OpId, MemberId)>);

/// What `node`'s daemon owes plain host `host`.
fn owed_to(node: &Node, host: &HostId) -> Option<Owed> {
    removals::owed(&node.fleet)
        .into_iter()
        .find(|(holder, ..)| *holder == Holder::Host(*host))
        .map(|(_, _, ops, also)| (ops.iter().map(Op::id).collect(), also))
}

#[tokio::test(flavor = "multi_thread")]
async fn a_held_removal_is_sent_again_when_it_turns_out_to_take_more() {
    let (http, _, _) = relay().await;
    let (a, c, x) = (
        Node::new(http, 1).await,
        Node::new(http, 3).await,
        Node::new(http, 9).await,
    );
    a.ask(Ask::Found);
    join(&a, &c);
    let (_dir, _agent, _stop, intro) = plain_host(http).await;
    let host = intro.id().unwrap();
    a.ask(Ask::AddHost {
        intro: sync::text(&intro.to_bytes().unwrap()),
        deny: Vec::new(),
    });
    // c admits x, not having heard of its own removal.
    join(&c, &x);
    a.ask(Ask::Remove {
        member: c.me(),
        also_members: Vec::new(),
        also_hosts: Vec::new(),
        reason: Reason::Compromised,
    });
    let (ops, also) = owed_to(&a, &host).expect("the removal is owed");
    assert_eq!(ops.len(), 1);
    assert!(also.is_empty(), "a has not heard of x yet");
    let removal = ops[0];
    // The agent takes it.
    a.fleet.held(Holder::Host(host), [removal], []).unwrap();
    assert!(owed_to(&a, &host).is_none());
    // a hears of x's admission: the removal takes x too, so it is owed
    // again, with x.
    a.fleet.receive(c.ops(), now_ms()).unwrap();
    let x_id = x.fleet.me();
    assert!(
        a.fleet
            .inner()
            .log
            .as_ref()
            .unwrap()
            .state()
            .tombstones
            .contains(&x_id)
    );
    let (ops, also) = owed_to(&a, &host).expect("owed again");
    assert_eq!(ops, [removal]);
    assert_eq!(also, [(removal, x_id)]);
    // Once the agent takes x with it, nothing more is owed, across a
    // restart too.
    a.fleet.held(Holder::Host(host), [removal], also).unwrap();
    assert!(owed_to(&a, &host).is_none());
    let again = Fleet::load(a.dir.path(), now_ms()).unwrap();
    assert_eq!(
        again.inner().held_also[&host][&removal],
        [x_id].into_iter().collect()
    );
}
