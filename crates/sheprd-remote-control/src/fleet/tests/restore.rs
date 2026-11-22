//! A saved log longer than the pending pool reads back whole.

use super::*;
use crate::control::fleet::{Access, MAX_PENDING};

#[tokio::test(flavor = "multi_thread")]
async fn a_log_longer_than_the_pending_pool_survives_a_restart() {
    let (http, _, _) = relay().await;
    let (a, b) = (Node::new(http, 1).await, Node::new(http, 2).await);
    assert_eq!(a.ask(Ask::Found), Answer::Done);
    join(&a, &b);
    let host = b
        .view()
        .hosts
        .iter()
        .find(|host| host.added_by == b.me())
        .unwrap()
        .id
        .clone();
    let host = sync::host(&host).unwrap();
    let member = a.fleet.me();
    // Signed in memory and saved once: well past the pool, each on the
    // one before.
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
                .draft(member, now_ms(), body)
                .sign(a.fleet.key())
                .unwrap();
            assert_eq!(log.insert(op, now_ms()).unwrap(), Inserted::Applied);
        }
        a.fleet.save(&inner).unwrap();
    }
    let (heads, count) = {
        let inner = a.fleet.inner();
        let log = inner.log.as_ref().unwrap();
        (log.heads().to_vec(), log.ops().count())
    };
    assert!(count > MAX_PENDING + 64);
    let again = Fleet::load(a.dir.path(), now_ms()).unwrap();
    let inner = again.inner();
    let log = inner.log.as_ref().unwrap();
    assert_eq!(log.ops().count(), count);
    assert_eq!(log.heads(), heads.as_slice());
    assert_eq!(log.pending(), 0);
}
