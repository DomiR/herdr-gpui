//! Phones in the fleet: a `Link` a live phone member signed for this
//! computer admits its device key to the API, and only that; the device
//! goes with the member or the link; and a phone asking to join is never
//! admitted without a person's approval.

use super::*;
use crate::control::fleet::{Body, Link, Method, Reason, Remove};
use crate::web::tests::fleet::{Phone, device_key};

/// An address no relay listens on: these tests never dial.
pub(super) fn nowhere() -> SocketAddr {
    "127.0.0.1:9".parse().unwrap()
}

pub(super) fn fleet_id(node: &Node) -> FleetId {
    node.fleet.inner().log.as_ref().unwrap().fleet()
}

pub(super) fn admit(node: &Node, phone: &Phone) {
    let (bundle, signature) = phone.join(&fleet_id(node));
    assert_eq!(
        node.ask(Ask::Admit {
            bundle,
            signature,
            method: Method::Qr,
        }),
        Answer::Done
    );
}

/// `phone` signs `body` and hands it to `node`, as `POST /api/v2/fleet/sync`
/// does.
pub(super) fn post(node: &Node, phone: &Phone, body: Body) -> usize {
    let op = {
        let inner = node.fleet.inner();
        phone.sign(inner.log.as_ref().unwrap(), body)
    };
    node.fleet.receive(vec![op], now_ms()).unwrap()
}

pub(super) fn own_host(node: &Node) -> HostId {
    let inner = node.fleet.inner();
    grants::own_host(inner.log.as_ref().unwrap().state(), &node.fleet.me(), None).unwrap()
}

pub(super) fn link(node: &Node, phone: &Phone, seed: u8) {
    let body = Body::Link(Link {
        host: own_host(node),
        device_key: device_key(seed),
    });
    assert_eq!(post(node, phone, body), 0);
}

pub(super) fn device(node: &Node, seed: u8) -> Option<crate::auth::DeviceRecord> {
    let (id, _) = links::device_id(&device_key(seed)).unwrap();
    node.state.auth().devices().get(&id).cloned()
}

pub(super) fn apply(node: &Node) -> bool {
    links::apply(&node.state, &node.fleet, now_ms()).unwrap()
}

#[tokio::test]
async fn a_phones_link_admits_its_key_to_the_api_only() {
    let a = Node::new(nowhere(), 1).await;
    assert_eq!(a.ask(Ask::Found), Answer::Done);
    let phone = Phone::new(7);
    admit(&a, &phone);
    // Admitted, with no link yet: no device.
    assert!(!apply(&a));
    link(&a, &phone, 20);
    assert!(apply(&a));
    let record = device(&a, 20).unwrap();
    assert_eq!(record.name, "phone7");
    assert_eq!(
        record.via.as_deref(),
        Some(format!("fleet {}", fleet_id(&a).short()).as_str())
    );
    assert!(!record.share_github);
    assert!(record.ssh.is_none());
    assert!(record.id.0.starts_with("fleet-"));
    // A phone is never a host's SSH user.
    grants::apply(&a.state, &a.fleet).unwrap();
    assert!(a.lines().is_empty());
    // Nothing more to do.
    assert!(!apply(&a));
}

#[tokio::test]
async fn a_link_to_another_host_admits_nothing_here() {
    let a = Node::new(nowhere(), 1).await;
    assert_eq!(a.ask(Ask::Found), Answer::Done);
    let phone = Phone::new(7);
    admit(&a, &phone);
    // A host the fleet does not know: the link names nothing live.
    let body = Body::Link(Link {
        host: HostId([5; 32]),
        device_key: device_key(20),
    });
    post(&a, &phone, body);
    assert!(!apply(&a));
    assert!(device(&a, 20).is_none());
}

#[tokio::test]
async fn removing_the_phone_revokes_its_device() {
    let a = Node::new(nowhere(), 1).await;
    assert_eq!(a.ask(Ask::Found), Answer::Done);
    let phone = Phone::new(7);
    admit(&a, &phone);
    link(&a, &phone, 20);
    apply(&a);
    assert!(device(&a, 20).is_some());
    a.ask(Ask::Remove {
        member: phone.bundle.id().to_string(),
        also_members: Vec::new(),
        also_hosts: Vec::new(),
        reason: Reason::Lost,
    });
    assert!(apply(&a));
    assert!(device(&a, 20).is_none());
}

#[tokio::test]
async fn a_new_link_replaces_the_old_device() {
    let a = Node::new(nowhere(), 1).await;
    assert_eq!(a.ask(Ask::Found), Answer::Done);
    let phone = Phone::new(7);
    admit(&a, &phone);
    link(&a, &phone, 20);
    apply(&a);
    link(&a, &phone, 21);
    assert!(apply(&a));
    assert!(device(&a, 20).is_none());
    assert!(device(&a, 21).is_some());
}

#[tokio::test]
async fn a_device_revoked_here_stays_revoked() {
    let a = Node::new(nowhere(), 1).await;
    assert_eq!(a.ask(Ask::Found), Answer::Done);
    let phone = Phone::new(7);
    admit(&a, &phone);
    link(&a, &phone, 20);
    apply(&a);
    let id = device(&a, 20).unwrap().id;
    assert!(a.state.auth().revoke(&id).unwrap());
    assert!(!apply(&a));
    assert!(device(&a, 20).is_none());
    // A device it never admitted is not its to remove.
    let (other, _) = links::device_id(&device_key(30)).unwrap();
    assert!(a.state.auth().devices().get(&other).is_none());
}

#[tokio::test]
async fn a_refusal_outlives_a_suspension() {
    let a = Node::new(nowhere(), 1).await;
    assert_eq!(a.ask(Ask::Found), Answer::Done);
    let phone = Phone::new(7);
    admit(&a, &phone);
    link(&a, &phone, 20);
    apply(&a);
    let id = device(&a, 20).unwrap().id;
    assert!(a.state.auth().revoke(&id).unwrap());
    // The phone is suspended a while, by a removal whose history has not
    // arrived, then the suspension lifts.
    let removal = OpId([7; 32]);
    a.fleet.inner().unconfirmed.insert(
        removal,
        Unconfirmed {
            signer: a.fleet.me(),
            targets: vec![phone.bundle.id()],
            since_ms: now_ms(),
            op: None,
        },
    );
    assert_eq!(a.fleet.suspended(), vec![phone.bundle.id()]);
    apply(&a);
    a.fleet.inner().unconfirmed.remove(&removal);
    assert!(a.fleet.suspended().is_empty());
    // Revoked here by hand: it stays revoked.
    assert!(!apply(&a));
    assert!(device(&a, 20).is_none());
}

#[tokio::test]
async fn a_suspension_lifted_restores_a_device_not_refused() {
    let a = Node::new(nowhere(), 1).await;
    assert_eq!(a.ask(Ask::Found), Answer::Done);
    let phone = Phone::new(7);
    admit(&a, &phone);
    link(&a, &phone, 20);
    apply(&a);
    let removal = OpId([7; 32]);
    a.fleet.inner().unconfirmed.insert(
        removal,
        Unconfirmed {
            signer: a.fleet.me(),
            targets: vec![phone.bundle.id()],
            since_ms: now_ms(),
            op: None,
        },
    );
    assert!(apply(&a));
    assert!(device(&a, 20).is_none());
    a.fleet.inner().unconfirmed.remove(&removal);
    assert!(apply(&a));
    assert!(device(&a, 20).is_some());
}

#[tokio::test]
async fn a_phone_removing_itself_ends_its_device() {
    let a = Node::new(nowhere(), 1).await;
    assert_eq!(a.ask(Ask::Found), Answer::Done);
    let phone = Phone::new(7);
    admit(&a, &phone);
    link(&a, &phone, 20);
    apply(&a);
    let body = Body::Remove(Remove {
        member: phone.bundle.id(),
        also_members: Vec::new(),
        also_hosts: Vec::new(),
        reason: Reason::Left,
    });
    assert_eq!(post(&a, &phone, body), 0);
    apply(&a);
    assert!(device(&a, 20).is_none());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_pending_join_is_never_admitted_on_its_own() {
    let (http, _, _) = relay().await;
    let mut a = Node::new(http, 1).await;
    assert_eq!(a.ask(Ask::Found), Answer::Done);
    a.start().await;
    let phone = Phone::new(7);
    let (_, signature) = phone.join(&fleet_id(&a));
    let signature = sync::bytes(&signature).unwrap().try_into().unwrap();
    // Asked from a phone paired here.
    let (asker, _) = crate::web::tests::signed_in(&a.state, 1);
    {
        let inner = a.fleet.inner();
        let standing = a
            .fleet
            .joins()
            .ask(
                inner.log.as_ref().unwrap(),
                phone.bundle.clone(),
                signature,
                (asker, "iPhone".into()),
                now_ms(),
            )
            .unwrap();
        assert_eq!(standing, joins::Standing::Pending);
    }
    // The daemon follows its fleet for a while: nothing admits the phone.
    a.fleet.bump();
    tokio::time::sleep(Duration::from_millis(300)).await;
    let member = phone.bundle.id();
    assert!(!a.fleet.inner().log.as_ref().unwrap().is_member(&member));
    let Answer::Joins { joins } = a.ask(Ask::Joins) else {
        panic!("no joins");
    };
    assert_eq!(joins.len(), 1);
    // Its own operations count for nothing while it waits.
    let body = Body::Link(Link {
        host: own_host(&a),
        device_key: device_key(20),
    });
    assert_eq!(post(&a, &phone, body), 1);
    assert!(device(&a, 20).is_none());
}
