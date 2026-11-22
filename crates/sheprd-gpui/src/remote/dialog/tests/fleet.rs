//! The approval step's Also add to your fleet: shown only while this
//! computer is a member, and an approval records the phone for that fleet
//! only when the switch was shown ticked.

use super::*;
use crate::fleet::Fleet;
use sheprd_remote_control::control::fleet::{Kind, MemberView, View};

fn claimed_daemon(_: &Path, request: &Request) -> Result<Response, control::Error> {
    Ok(match request {
        Request::Pair => Response::Offer(offer()),
        Request::Pairing { .. } => Response::Pairing(PairingState::Claimed {
            device_name: "iPhone".into(),
            code: "428913".into(),
            app: None,
            ssh: false,
            fleet: None,
        }),
        Request::Approve { allow: true, .. } => Response::Pairing(PairingState::Approved {
            device: DeviceId("d1".into()),
        }),
        _ => Response::Done,
    })
}

fn member_view() -> View {
    View {
        fleet: "f".repeat(64),
        me: "a".into(),
        members: vec![MemberView {
            id: "a".into(),
            name: "studio".into(),
            kind: Kind::Computer,
            admitted_by: None,
        }],
        hosts: Vec::new(),
        removed: 0,
        equivocating: Vec::new(),
        suspended: Vec::new(),
        pending: 0,
        heads: Vec::new(),
        hosted: None,
    }
}

fn allow(view: &Entity<SheprdWindow>, cx: &mut VisualTestContext) {
    open(view, cx);
    wait(cx);
    draw(cx);
    let allow = cx.debug_bounds("remote-pair-allow").unwrap().center();
    cx.simulate_click(allow, Modifiers::default());
    wait(cx);
    wait(cx);
    assert!(matches!(phase(view, cx), Phase::Approved { .. }));
}

#[gpui::test]
fn outside_a_fleet_the_line_is_hidden_and_approval_records_nothing(cx: &mut TestAppContext) {
    let (view, cx) = window(cx, true, claimed_daemon);
    cx.update(|_, cx| Fleet::install(cx));
    open(&view, cx);
    wait(cx);
    draw(cx);
    assert!(cx.debug_bounds("remote-pair-fleet").is_none());
    // The switch defaults to ticked, yet a hidden switch grants nothing.
    let allow = cx.debug_bounds("remote-pair-allow").unwrap().center();
    cx.simulate_click(allow, Modifiers::default());
    wait(cx);
    wait(cx);
    assert!(matches!(phase(&view, cx), Phase::Approved { .. }));
    cx.update(|_, cx| Fleet::set_view(cx, Some(member_view())));
    wait(cx);
    assert!(cx.update(|_, cx| Fleet::expected(cx)).is_empty());
}

#[gpui::test]
fn a_member_records_the_approved_phone_for_its_fleet(cx: &mut TestAppContext) {
    let (view, cx) = window(cx, true, claimed_daemon);
    cx.update(|_, cx| {
        Fleet::install(cx);
        Fleet::set_view(cx, Some(member_view()));
    });
    allow(&view, cx);
    assert_eq!(
        cx.update(|_, cx| Fleet::expected(cx)),
        vec![(DeviceId("d1".into()), "f".repeat(64))]
    );
}

#[gpui::test]
fn unticking_the_line_records_nothing(cx: &mut TestAppContext) {
    let (view, cx) = window(cx, true, claimed_daemon);
    cx.update(|_, cx| {
        Fleet::install(cx);
        Fleet::set_view(cx, Some(member_view()));
    });
    open(&view, cx);
    wait(cx);
    draw(cx);
    let switch = cx.debug_bounds("remote-pair-fleet").unwrap().center();
    cx.simulate_click(switch, Modifiers::default());
    draw(cx);
    let allow = cx.debug_bounds("remote-pair-allow").unwrap().center();
    cx.simulate_click(allow, Modifiers::default());
    wait(cx);
    wait(cx);
    assert!(matches!(phase(&view, cx), Phase::Approved { .. }));
    assert!(cx.update(|_, cx| Fleet::expected(cx)).is_empty());
}
