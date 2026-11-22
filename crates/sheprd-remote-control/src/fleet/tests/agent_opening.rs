//! A plain host's agent reads a fleet session's opening, the purpose byte
//! and proof, as a member's daemon does: its record keeps the opening, and
//! a removal dialed to it sends it.

use super::agent::plain_host;
use super::*;
use crate::control::fleet::sync;
use crate::desktop::record::Record;

#[tokio::test(flavor = "multi_thread")]
async fn a_plain_hosts_record_carries_the_opening_its_agent_reads() {
    let (http, _, _) = relay().await;
    let (a, b) = (Node::new(http, 1).await, Node::new(http, 2).await);
    a.ask(Ask::Found);
    join(&a, &b);
    let (_dir, _agent, _stop, intro) = plain_host(http).await;
    let route = intro.record.route.clone();
    a.ask(Ask::AddHost {
        intro: sync::text(&intro.to_bytes().unwrap()),
        deny: Vec::new(),
    });
    b.fleet.receive(a.ops(), now_ms()).unwrap();
    grants::apply(&b.state, &b.fleet).unwrap();
    let probe = Record {
        computer: String::new(),
        identity: String::new(),
        relays: Vec::new(),
        route,
        fleet: true,
        host: None,
        opening: false,
    };
    let kept = Record::load(b.dir.path(), &probe.id().unwrap()).unwrap();
    assert!(kept.fleet);
    assert!(kept.opening);
}
