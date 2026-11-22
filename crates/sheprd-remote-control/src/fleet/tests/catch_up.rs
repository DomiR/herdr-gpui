//! A member far behind takes every operation handed to it at once, past
//! the bound on operations waiting for their parents.

use super::*;
use crate::control::fleet::{Access, MAX_PENDING};

#[tokio::test(flavor = "multi_thread")]
async fn a_large_update_applies_whole() {
    let (http, _, _) = relay().await;
    let (a, b, c) = (
        Node::new(http, 1).await,
        Node::new(http, 2).await,
        Node::new(http, 3).await,
    );
    assert_eq!(a.ask(Ask::Found), Answer::Done);
    join(&a, &b);
    let host = grants::own_host(
        a.fleet.inner().log.as_ref().unwrap().state(),
        &a.fleet.me(),
        None,
    )
    .unwrap();
    let member = b.fleet.me();
    {
        let mut inner = a.fleet.inner();
        let log = inner.log.as_mut().unwrap();
        for turn in 0..MAX_PENDING + 64 {
            let body = Body::Access(Access {
                host,
                member,
                allow: turn % 2 == 1,
                rev: sync::next_rev(log.state(), host, member),
            });
            let op = log
                .draft(a.fleet.me(), now_ms(), body)
                .sign(a.fleet.key())
                .unwrap();
            log.insert(op, now_ms()).unwrap();
        }
    }
    // Handed in reverse, newest first, to b catching up and to c joining.
    let mut ops = a.ops();
    ops.reverse();
    assert_eq!(b.fleet.receive(ops.clone(), now_ms()).unwrap(), 0);
    let heads = a.fleet.inner().log.as_ref().unwrap().heads().to_vec();
    let inner = b.fleet.inner();
    assert_eq!(inner.log.as_ref().unwrap().heads(), heads.as_slice());
    assert_eq!(inner.log.as_ref().unwrap().pending(), 0);
    drop(inner);
    // c, admitted, joins from the same long log.
    let fleet = a.view().fleet;
    let Answer::Bundle { bundle, signature } = c.ask(Ask::Bundle { fleet }) else {
        panic!("no bundle");
    };
    a.ask(Ask::Admit {
        bundle,
        signature,
        method: crate::control::fleet::Method::Ssh,
    });
    let mut ops = a.ops();
    ops.reverse();
    assert_eq!(c.fleet.receive(ops, now_ms()).unwrap(), 0);
    assert_eq!(
        c.fleet.inner().log.as_ref().unwrap().heads(),
        a.fleet.inner().log.as_ref().unwrap().heads()
    );
}
