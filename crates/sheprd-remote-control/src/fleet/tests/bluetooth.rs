//! Joining the fleet over Bluetooth in one step: a desktop pairs with a
//! nearby computer through its relay on loopback, with the control
//! socket's own nearby, pairing, and approval steps, and leaves a member
//! of the fleet one of them was in, or paired only.

use super::*;
use crate::{
    control::{
        PairingId, PairingState, Request, Response,
        fleet::{
            Method,
            bluetooth::{ApprovedFleet, ClaimFleet, Hint, Joining, pairing_digest},
            sync,
        },
    },
    desktop::{
        self,
        for_tests::{Allowed, Claimant, Exchange, Opened, checked},
        keys::Keys,
    },
};

/// The other computer's Sheprd: the control socket's answer to `request`.
fn control(node: &Node, request: Request) -> Response {
    let (stop, _) = watch::channel(false);
    crate::server::answer_for_tests(&node.state, &stop, request)
}

/// Opens Pair device on `node`.
fn pair_device(node: &Node) -> PairingId {
    node.state
        .auth()
        .mint(
            now_ms(),
            &[crate::web::tests::ORIGIN.to_owned()],
            &node.state.fingerprint,
        )
        .unwrap()
        .id
}

/// The code exchange with `node` as Sheprd carries it over Bluetooth.
fn exchange(node: &Node) -> Opened {
    #[derive(serde::Deserialize)]
    #[serde(rename_all = "snake_case", tag = "type")]
    enum Message {
        Start { public_key: String },
        Reveal { session: String, nonce: String },
    }
    let carry = |frame: &[u8]| -> Vec<u8> {
        let request = match serde_json::from_slice(frame).unwrap() {
            Message::Start { public_key } => Request::NearbyStart { public_key },
            Message::Reveal { session, nonce } => Request::NearbyReveal { session, nonce },
        };
        match control(node, request) {
            Response::NearbyStarted(started) => serde_json::to_vec(&started).unwrap(),
            Response::NearbyRevealed(revealed) => serde_json::to_vec(&revealed).unwrap(),
            other => panic!("{other:?}"),
        }
    };
    let (mut exchange, start) = Exchange::start().unwrap();
    let reveal = exchange.reveal(&carry(&start)).unwrap();
    exchange.open(&carry(&reveal)).unwrap()
}

/// The person on `node` allows the claim, with the fleet line agreeing to
/// `agreed`; what the dialog was shown of the fleet.
async fn allow(
    node: &Node,
    pairing: &PairingId,
    agreed: Option<String>,
) -> Option<crate::control::fleet::bluetooth::Asked> {
    for _ in 0..200 {
        let state = control(
            node,
            Request::Pairing {
                id: pairing.clone(),
            },
        );
        if let Response::Pairing(PairingState::Claimed { code, fleet, .. }) = state {
            let answered = control(
                node,
                Request::Approve {
                    id: pairing.clone(),
                    allow: true,
                    code,
                    hosts: Vec::new(),
                    share_github: true,
                    fleet: agreed,
                },
            );
            assert!(
                matches!(answered, Response::Pairing(PairingState::Approved { .. })),
                "{answered:?}"
            );
            return fleet;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("never claimed");
}

/// Claims `opened` as `node`'s desktop-pair would, with its own keys.
async fn claim(
    node: &Node,
    opened: &Opened,
    fleet: Option<ClaimFleet>,
) -> desktop::Result<Allowed> {
    let keys = Keys::load_or_create(node.dir.path()).unwrap();
    let ssh_key = desktop::keys::ssh_public_key(node.dir.path()).unwrap();
    let claimant = Claimant {
        keys: &keys,
        name: "Laptop",
        ssh_key: &ssh_key,
        fleet: fleet.as_ref(),
    };
    desktop::for_tests::claim(opened, &claimant, |_| {}).await
}

/// `node`'s own bundle for `fleet`, bound to the pairing `opened` holds.
fn joining(node: &Node, fleet: &str, opened: &Opened) -> Joining {
    let Answer::Joining(joining) = node.ask(Ask::Joining {
        fleet: fleet.to_owned(),
        pairing: sync::text(&pairing_digest(&opened.token)),
    }) else {
        panic!("no joining");
    };
    joining
}

fn hint(node: &Node) -> Hint {
    crate::fleet::bluetooth::hint(&node.fleet).unwrap()
}

/// Two computers on one relay, both listening.
async fn pair() -> (Node, Node) {
    let (http, _, _) = relay().await;
    let (mut a, mut b) = (Node::new(http, 1).await, Node::new(http, 2).await);
    a.start().await;
    b.start().await;
    (a, b)
}

#[tokio::test(flavor = "multi_thread")]
async fn a_desktop_in_no_fleet_joins_the_fleet_of_the_computer_it_pairs_with() {
    let (a, b) = pair().await;
    assert_eq!(b.ask(Ask::Found), Answer::Done);
    let fleet = b.view().fleet;
    let pairing = pair_device(&b);

    let opened = exchange(&b);
    assert_eq!(opened.fleet.as_ref().map(|hint| &hint.fleet), Some(&fleet));
    let asked = ClaimFleet::Join(joining(&a, &fleet, &opened));
    let (allowed, shown) = tokio::join!(
        claim(&a, &opened, Some(asked)),
        allow(&b, &pairing, Some(fleet.clone()))
    );
    assert_eq!(
        shown,
        Some(crate::control::fleet::bluetooth::Asked::Join {
            fleet: fleet.clone()
        })
    );
    let allowed = allowed.unwrap();
    assert_eq!(
        allowed.fleet,
        Some(ApprovedFleet::Joined {
            fleet: fleet.clone()
        })
    );

    // Admitted by Bluetooth, before the log is handed over.
    let admitted = b.view();
    assert!(admitted.members.iter().any(|member| member.id == a.me()));
    let admit = b.ops().into_iter().find_map(|op| match op.body() {
        Body::Admit(admit) => Some(admit.method),
        _ => None,
    });
    assert_eq!(admit, Some(Method::Bluetooth));

    // The log handed over admits a, which takes it, and b lets a in.
    a.fleet.receive(b.ops(), now_ms()).unwrap();
    assert_eq!(a.view().fleet, fleet);
    let short = a.fleet.me().short();
    until(10, || b.lines().iter().any(|line| line.contains(&short))).await;
    let short = b.fleet.me().short();
    until(10, || a.lines().iter().any(|line| line.contains(&short))).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_computer_in_no_fleet_joins_the_fleet_of_the_desktop_that_invites_it() {
    let (a, b) = pair().await;
    assert_eq!(a.ask(Ask::Found), Answer::Done);
    let fleet = a.view().fleet;
    let pairing = pair_device(&b);

    let opened = exchange(&b);
    assert_eq!(opened.fleet, None);
    let invite = ClaimFleet::Invite(hint(&a));
    let (allowed, shown) = tokio::join!(
        claim(&a, &opened, Some(invite.clone())),
        allow(&b, &pairing, Some(fleet.clone()))
    );
    assert_eq!(
        shown,
        Some(crate::control::fleet::bluetooth::Asked::Invite(hint(&a)))
    );
    let Some(ApprovedFleet::Joining(joining)) = allowed.unwrap().fleet else {
        panic!("no bundle came back");
    };

    // a admits b, as its Sheprd does with what the child checked, and
    // hands the log over.
    assert_eq!(
        a.ask(Ask::Admit {
            bundle: joining.bundle,
            signature: joining.signature,
            method: Method::Bluetooth,
        }),
        Answer::Done
    );
    b.fleet.receive(a.ops(), now_ms()).unwrap();
    assert_eq!(b.view().fleet, fleet);
    let short = b.fleet.me().short();
    until(10, || a.lines().iter().any(|line| line.contains(&short))).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn an_allow_without_the_fleet_line_pairs_only() {
    let (a, b) = pair().await;
    assert_eq!(b.ask(Ask::Found), Answer::Done);
    let fleet = b.view().fleet;
    let pairing = pair_device(&b);
    let opened = exchange(&b);
    let asked = ClaimFleet::Join(joining(&a, &fleet, &opened));
    let (allowed, _) = tokio::join!(claim(&a, &opened, Some(asked)), allow(&b, &pairing, None));
    assert_eq!(allowed.unwrap().fleet, None);
    assert!(!b.view().members.iter().any(|member| member.id == a.me()));
}

#[tokio::test(flavor = "multi_thread")]
async fn an_allow_for_a_fleet_the_dialog_did_not_show_admits_nobody() {
    let (a, b) = pair().await;
    assert_eq!(b.ask(Ask::Found), Answer::Done);
    let fleet = b.view().fleet;
    let pairing = pair_device(&b);
    let opened = exchange(&b);
    let asked = ClaimFleet::Join(joining(&a, &fleet, &opened));
    let elsewhere = "ab".repeat(32);
    let (allowed, _) = tokio::join!(
        claim(&a, &opened, Some(asked)),
        allow(&b, &pairing, Some(elsewhere))
    );
    assert_eq!(allowed.unwrap().fleet, None);
    assert!(!b.view().members.iter().any(|member| member.id == a.me()));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_computer_in_another_fleet_pairs_without_joining() {
    let (a, b) = pair().await;
    assert_eq!(a.ask(Ask::Found), Answer::Done);
    assert_eq!(b.ask(Ask::Found), Answer::Done);
    let theirs = a.view().fleet;
    let ours = b.view().fleet;
    let pairing = pair_device(&b);
    let opened = exchange(&b);
    // b names its own fleet; a, in another, invites b into its own.
    assert_eq!(opened.fleet.as_ref().map(|hint| &hint.fleet), Some(&ours));
    let (allowed, _) = tokio::join!(
        claim(&a, &opened, Some(ClaimFleet::Invite(hint(&a)))),
        allow(&b, &pairing, Some(theirs))
    );
    assert_eq!(allowed.unwrap().fleet, None);
    assert_eq!(b.view().fleet, ours);
    // Nor does a computer in a fleet sign itself into another.
    assert!(matches!(
        ask::answer(
            &a.state,
            &a.fleet,
            Ask::Joining {
                fleet: ours,
                pairing: sync::text(&[0; 32]),
            },
            now_ms()
        ),
        Err(Error::InFleet)
    ));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_join_with_another_computers_bundle_is_refused_before_the_token_is_spent() {
    let (http, _, _) = relay().await;
    let (mut a, mut b, c) = (
        Node::new(http, 1).await,
        Node::new(http, 2).await,
        Node::new(http, 3).await,
    );
    a.start().await;
    b.start().await;
    assert_eq!(b.ask(Ask::Found), Answer::Done);
    let fleet = b.view().fleet;
    let pairing = pair_device(&b);
    let opened = exchange(&b);

    // c's own bundle, properly bound to this pairing, but a claims with
    // its keys: admitting it would let in a key that compared no code.
    let stolen = ClaimFleet::Join(joining(&c, &fleet, &opened));
    let refused = claim(&a, &opened, Some(stolen)).await;
    assert!(
        matches!(refused, Err(desktop::Error::Claim(_))),
        "{refused:?}"
    );

    // Refused before the token was spent: the dialog still waits, and
    // nobody joined.
    assert!(matches!(
        control(&b, Request::Pairing { id: pairing }),
        Response::Pairing(PairingState::Waiting { .. })
    ));
    assert!(!b.view().members.iter().any(|member| member.id == c.me()));
    assert!(!b.view().members.iter().any(|member| member.id == a.me()));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_joining_bound_to_another_pairing_is_refused() {
    let (a, b) = pair().await;
    assert_eq!(b.ask(Ask::Found), Answer::Done);
    let fleet = b.view().fleet;
    let first = pair_device(&b);
    let earlier = exchange(&b);
    let replayed = joining(&a, &fleet, &earlier);
    control(&b, Request::CancelPairing { id: first });

    let _pairing = pair_device(&b);
    let opened = exchange(&b);
    let refused = claim(&a, &opened, Some(ClaimFleet::Join(replayed))).await;
    assert!(
        matches!(refused, Err(desktop::Error::Claim(_))),
        "{refused:?}"
    );
    assert!(!b.view().members.iter().any(|member| member.id == a.me()));
}

#[tokio::test(flavor = "multi_thread")]
async fn an_answer_with_another_computers_bundle_is_not_handed_on() {
    let (http, _, _) = relay().await;
    let (mut a, mut b, c) = (
        Node::new(http, 1).await,
        Node::new(http, 2).await,
        Node::new(http, 3).await,
    );
    a.start().await;
    b.start().await;
    assert_eq!(a.ask(Ask::Found), Answer::Done);
    let fleet = a.view().fleet;
    let _pairing = pair_device(&b);
    let opened = exchange(&b);
    let invite = ClaimFleet::Invite(hint(&a));

    // b's honest answer passes; c's bundle, bound to the same pairing, does
    // not name the authority, route, and identity the code covered.
    let honest = joining(&b, &fleet, &opened);
    assert_eq!(
        checked(
            &opened,
            Some(&invite),
            Some(ApprovedFleet::Joining(honest.clone()))
        ),
        Some(ApprovedFleet::Joining(honest))
    );
    let substituted = joining(&c, &fleet, &opened);
    assert_eq!(
        checked(
            &opened,
            Some(&invite),
            Some(ApprovedFleet::Joining(substituted))
        ),
        None
    );
    // Nor is an answer to a question the claim never asked.
    assert_eq!(
        checked(
            &opened,
            None,
            Some(ApprovedFleet::Joined {
                fleet: fleet.clone()
            })
        ),
        None
    );
    assert_eq!(
        checked(
            &opened,
            Some(&invite),
            Some(ApprovedFleet::Joined { fleet })
        ),
        None
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_claim_without_the_token_learns_nothing_of_the_fleet() {
    use crate::web::tests::{post, send};
    let (a, b) = pair().await;
    let _pairing = pair_device(&b);
    let keys = Keys::load_or_create(a.dir.path()).unwrap();
    let ssh_key = desktop::keys::ssh_public_key(a.dir.path()).unwrap();
    let body = |fleet: serde_json::Value| {
        serde_json::json!({
            "token": sync::text(&[7; 32]),
            "name": "Laptop",
            "public_key": "key",
            "relay": {
                "dial": crate::auth::encode(&sheprd_relay::client::public(&keys.dial)),
                "tunnel": crate::auth::encode(&keys.tunnel.identity().x25519()),
            },
            "ssh": { "public_key": ssh_key },
            "fleet": fleet,
        })
    };
    // b is in no fleet: a fleet part read before the token would say so.
    let plain = send(
        &b.state,
        post("/api/pair/claim", body(serde_json::Value::Null)),
    )
    .await;
    let joining = serde_json::json!({
        "join": { "bundle": "AAAA", "signature": "AAAA", "binding": "AAAA" }
    });
    let asked = send(&b.state, post("/api/pair/claim", body(joining))).await;
    assert_eq!((plain.0, plain.2), (asked.0, asked.2));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_handover_naming_one_fleet_never_adopts_another() {
    let (http, _, _) = relay().await;
    let (a, b, c) = (
        Node::new(http, 1).await,
        Node::new(http, 2).await,
        Node::new(http, 3).await,
    );
    assert_eq!(b.ask(Ask::Found), Answer::Done);
    assert_eq!(c.ask(Ask::Found), Answer::Done);
    let fleet = b.view().fleet;
    let other = c.view().fleet;
    // a once signed a join for c's fleet, which c admitted, and a never
    // took that log.
    let Answer::Bundle { bundle, signature } = a.ask(Ask::Bundle {
        fleet: other.clone(),
    }) else {
        panic!("no bundle");
    };
    assert_eq!(
        c.ask(Ask::Admit {
            bundle,
            signature,
            method: Method::Ssh,
        }),
        Answer::Done
    );
    // b, asked for its own fleet's log, carries c's history instead.
    let carried = sync::Apply {
        fleet: Some(fleet.clone()),
        ops: sync::encode_ops(&c.ops()),
        ..sync::Apply::default()
    };
    let _ = a.fleet.apply(&carried, now_ms());
    assert!(
        ask::view(&a.fleet).is_none(),
        "a took a fleet it did not name"
    );
    // The fleet it named, once that admits it, is taken.
    let Answer::Bundle { bundle, signature } = a.ask(Ask::Bundle {
        fleet: fleet.clone(),
    }) else {
        panic!("no bundle");
    };
    b.ask(Ask::Admit {
        bundle,
        signature,
        method: Method::Bluetooth,
    });
    let handed = sync::Apply {
        fleet: Some(fleet.clone()),
        ops: sync::encode_ops(&b.ops()),
        ..sync::Apply::default()
    };
    a.fleet.apply(&handed, now_ms()).unwrap();
    assert_eq!(a.view().fleet, fleet);
}
