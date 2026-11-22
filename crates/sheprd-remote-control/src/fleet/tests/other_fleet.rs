//! A caller handing over one fleet's history never gets another's back,
//! even when the other became this computer's log in the meantime.

use super::*;
use crate::control::fleet::sync::Apply;

#[tokio::test(flavor = "multi_thread")]
async fn a_handover_of_another_fleet_never_answers_with_this_ones_history() {
    let (http, _, _) = relay().await;
    let (a, c, b) = (
        Node::new(http, 1).await,
        Node::new(http, 3).await,
        Node::new(http, 2).await,
    );
    assert_eq!(a.ask(Ask::Found), Answer::Done);
    assert_eq!(c.ask(Ask::Found), Answer::Done);
    // b joins a's fleet, as a handover that won a race would leave it.
    join(&a, &b);
    let first = a.view().fleet;
    // c's history arrives after: named, or only by its operations.
    let named = Apply {
        fleet: Some(c.view().fleet),
        ops: sync::encode_ops(&c.ops()),
        ..Apply::default()
    };
    let unnamed = Apply {
        fleet: None,
        ..named.clone()
    };
    for apply in [named, unnamed] {
        assert!(matches!(
            b.fleet.apply(&apply, now_ms()),
            Err(Error::OtherFleet)
        ));
    }
    // A sync that names nothing and carries nothing is still answered.
    let applied = b.fleet.apply(&Apply::default(), now_ms()).unwrap();
    assert_eq!(applied.fleet, first);
}
