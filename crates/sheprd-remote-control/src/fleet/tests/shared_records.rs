//! A shared Sheprd host is no member and cannot sign its own record: a
//! member syncing it carries its newer, CA-signed record into the log.

use super::*;
use crate::control::fleet::sync::Apply;

/// `a`'s fleet shares `host`, which takes the log.
fn share(a: &Node, host: &Node) {
    let Answer::Intro { intro } = host.ask(Ask::Intro) else {
        panic!("no intro");
    };
    a.ask(Ask::AddHost {
        intro,
        deny: Vec::new(),
    });
    host.ask(Ask::Apply(Apply {
        heads: a.view().heads,
        ops: sync::encode_ops(&a.ops()),
        ..Apply::default()
    }));
}

fn record_keys(a: &Node, host: &Node) -> Vec<String> {
    let id = host.fleet.host().unwrap();
    a.fleet.inner().log.as_ref().unwrap().state().hosts[&id]
        .record
        .ssh_host_keys
        .clone()
}

#[tokio::test(flavor = "multi_thread")]
async fn a_member_carries_a_shared_hosts_moved_record() {
    let (http, _, _) = relay().await;
    let (a, host) = (Node::new(http, 1).await, Node::new(http, 5).await);
    assert_eq!(a.ask(Ask::Found), Answer::Done);
    share(&a, &host);
    let before = record_keys(&a, &host);
    // Unchanged: nothing is signed.
    let heads = a.view().heads;
    let Answer::Intro { intro } = host.ask(Ask::Intro) else {
        panic!("no intro");
    };
    assert_eq!(a.ask(Ask::Seen { intro }), Answer::Done);
    assert_eq!(a.view().heads, heads);
    // Its SSH host key moved.
    std::fs::write(
        host.dir.path().join("etc-ssh/ssh_host_ed25519_key.pub"),
        format!(
            "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAI{} root@node\n",
            "Z".repeat(43)
        ),
    )
    .unwrap();
    tokio::time::sleep(Duration::from_millis(5)).await;
    let Answer::Intro { intro } = host.ask(Ask::Intro) else {
        panic!("no intro");
    };
    assert_eq!(a.ask(Ask::Seen { intro }), Answer::Done);
    let after = record_keys(&a, &host);
    assert_ne!(after, before);
    assert!(after[0].contains(&"Z".repeat(43)), "{after:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_members_record_is_never_carried_for_it() {
    let (http, _, _) = relay().await;
    let (a, b) = (Node::new(http, 1).await, Node::new(http, 2).await);
    assert_eq!(a.ask(Ask::Found), Answer::Done);
    join(&a, &b);
    std::fs::write(
        b.dir.path().join("etc-ssh/ssh_host_ed25519_key.pub"),
        format!(
            "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAI{} root@node\n",
            "Y".repeat(43)
        ),
    )
    .unwrap();
    tokio::time::sleep(Duration::from_millis(5)).await;
    let heads = a.view().heads;
    let Answer::Intro { intro } = b.ask(Ask::Intro) else {
        panic!("no intro");
    };
    // b signs its own record; a carries nothing for it.
    assert_eq!(a.ask(Ask::Seen { intro }), Answer::Done);
    assert_eq!(a.view().heads, heads);
}

/// What `node` asks its daemon for `host`'s removal, from `heads`.
fn removal_for(node: &Node, host: &str, heads: Vec<String>) -> sync::Applied {
    let Answer::Applied(applied) = node.ask(Ask::RemovalFor {
        host: host.to_owned(),
        heads,
    }) else {
        panic!("not applied");
    };
    applied
}

fn ids(applied: &sync::Applied) -> Vec<OpId> {
    sync::decode_ops(&applied.ops)
        .unwrap()
        .iter()
        .map(Op::id)
        .collect()
}

#[tokio::test(flavor = "multi_thread")]
async fn a_removed_host_is_handed_its_removal_and_nothing_after_it() {
    let (http, _, _) = relay().await;
    let (a, host) = (Node::new(http, 1).await, Node::new(http, 5).await);
    assert_eq!(a.ask(Ask::Found), Answer::Done);
    share(&a, &host);
    let id = host.fleet.host().unwrap().to_string();
    let own = a
        .view()
        .hosts
        .iter()
        .find(|shared| shared.id != id)
        .unwrap()
        .id
        .clone();
    let toggle = |allow| {
        a.ask(Ask::Access {
            host: own.clone(),
            member: a.me(),
            allow,
        })
    };
    // One change the host has not seen, its removal, then one after it.
    let held_before: Vec<OpId> = host.ops().iter().map(Op::id).collect();
    toggle(false);
    assert_eq!(
        a.ask(Ask::RemovedHosts),
        Answer::Hosts { hosts: Vec::new() }
    );
    a.ask(Ask::RemoveHost { host: id.clone() });
    toggle(true);
    assert_eq!(
        a.ask(Ask::RemovedHosts),
        Answer::Hosts {
            hosts: vec![id.clone()]
        }
    );
    let heads: Vec<String> = host.view().heads;
    let part = removal_for(&a, &id, heads);
    let sent = ids(&part);
    // The change before it (an ancestor) and the removal: nothing after.
    assert_eq!(sent.len(), 2, "{sent:?}");
    // The change after the removal is the log's only head.
    let after = a.fleet.inner().log.as_ref().unwrap().heads()[0];
    assert!(!sent.contains(&after));
    assert!(sent.iter().all(|op| !held_before.contains(op)));
    // Handed over, the host lets nobody in; asked again, there is nothing
    // left to send, which is when Sheprd stops syncing it.
    let Answer::Applied(answer) = host.ask(Ask::Apply(Apply {
        fleet: Some(a.view().fleet),
        heads: part.heads.clone(),
        ops: part.ops.clone(),
        ..Apply::default()
    })) else {
        panic!("not applied");
    };
    assert!(!grants::still_allowed(&host.fleet, &a.fleet.me()));
    grants::apply(&host.state, &host.fleet).unwrap();
    assert!(host.lines().is_empty(), "{:?}", host.lines());
    assert!(removal_for(&a, &id, answer.heads).ops.is_empty());
    assert!(
        !host
            .fleet
            .inner()
            .log
            .as_ref()
            .unwrap()
            .ops()
            .any(|op| op.id() == after)
    );
    // It forgets the fleet.
    assert!(!host.fleet.in_fleet().unwrap());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_removed_members_own_host_is_no_removed_host() {
    let (http, _, _) = relay().await;
    let (a, b) = (Node::new(http, 1).await, Node::new(http, 2).await);
    assert_eq!(a.ask(Ask::Found), Answer::Done);
    join(&a, &b);
    a.ask(Ask::Remove {
        member: b.me(),
        also_members: Vec::new(),
        also_hosts: Vec::new(),
        reason: crate::control::fleet::Reason::Lost,
    });
    assert_eq!(
        a.ask(Ask::RemovedHosts),
        Answer::Hosts { hosts: Vec::new() }
    );
}
