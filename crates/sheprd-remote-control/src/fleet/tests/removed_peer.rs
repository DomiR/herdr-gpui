//! A host removed while its sync answer was on its way gets nothing made
//! after: the export checks the host Sheprd proved, under the lock it is
//! read under.

use super::*;
use crate::control::fleet::{HostKind, sync::Apply};

#[tokio::test(flavor = "multi_thread")]
async fn a_host_removed_mid_sync_is_exported_nothing() {
    let (http, _, _) = relay().await;
    let (a, host) = (Node::new(http, 1).await, Node::new(http, 5).await);
    assert_eq!(a.ask(Ask::Found), Answer::Done);
    let Answer::Intro { intro } = host.ask(Ask::Intro) else {
        panic!("no intro");
    };
    a.ask(Ask::AddHost {
        intro,
        deny: Vec::new(),
    });
    let id = a
        .view()
        .hosts
        .into_iter()
        .find(|shared| shared.added_by == a.me() && shared.kind != HostKind::Member)
        .unwrap()
        .id;
    let export = |peer: &str| {
        a.fleet.apply(
            &Apply {
                fleet: Some(a.view().fleet),
                peer: Some(peer.to_owned()),
                ..Apply::default()
            },
            now_ms(),
        )
    };
    // Held: exported as any sync.
    assert!(!export(&id).unwrap().ops.is_empty());
    // Removed while its answer was on its way.
    a.ask(Ask::RemoveHost { host: id.clone() });
    assert!(matches!(export(&id), Err(Error::HostRemoved)));
    // A member's own host is still exported to.
    let own = a
        .view()
        .hosts
        .into_iter()
        .find(|shared| shared.kind == HostKind::Member)
        .unwrap()
        .id;
    assert!(export(&own).is_ok());
}
