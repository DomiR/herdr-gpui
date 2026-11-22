//! Waiting to join: a phone is admitted only on a second click here, or
//! when the pairing that approved that very device ticked Also add to
//! your fleet.
use super::*;
use sheprd_remote_control::control::{
    DeviceId,
    fleet::{Method, sync::Join},
};

fn join(member: &str, device: &str) -> Join {
    Join {
        member: member.into(),
        name: format!("phone {member}"),
        kind: Kind::Phone,
        bundle: format!("bundle-{member}"),
        signature: format!("signature-{member}"),
        device: "iPhone".into(),
        device_id: DeviceId(device.into()),
        received_ms: 1,
    }
}

fn admit(member: &str) -> Ask {
    Ask::Admit {
        bundle: format!("bundle-{member}"),
        signature: format!("signature-{member}"),
        method: Method::Qr,
    }
}

fn reload(cx: &mut VisualTestContext) {
    cx.update(|_, cx| Fleet::refresh(cx));
    cx.run_until_parked();
    draw(cx);
}

#[gpui::test]
fn a_join_is_admitted_on_the_second_click(cx: &mut TestAppContext) {
    let _serial = serial();
    let cx = page(cx, true);
    assert!(cx.debug_bounds("fleet-join-0").is_none());
    JOINS.lock().unwrap().push(join("j1", "d1"));
    reload(cx);
    assert!(cx.debug_bounds("fleet-join-0").is_some());
    click(cx, "fleet-join-0-admit");
    assert!(
        ASKED.lock().unwrap().is_empty(),
        "nothing is signed on the first click"
    );
    click(cx, "fleet-join-0-admit");
    cx.run_until_parked();
    assert_eq!(*ASKED.lock().unwrap(), vec![admit("j1")]);
}

#[gpui::test]
fn a_pairing_approval_admits_only_the_device_it_approved(cx: &mut TestAppContext) {
    let _serial = serial();
    let cx = page(cx, true);
    JOINS
        .lock()
        .unwrap()
        .extend([join("j1", "d1"), join("j2", "d2")]);
    cx.update(|_, cx| Fleet::expect(cx, DeviceId("d2".into()), "f".repeat(64)));
    cx.run_until_parked();
    draw(cx);
    assert_eq!(*ASKED.lock().unwrap(), vec![admit("j2")]);
    // The other phone still waits for a person, and the approval is spent.
    assert!(cx.debug_bounds("fleet-join-0").is_some());
    assert!(cx.debug_bounds("fleet-join-1").is_none());
    ASKED.lock().unwrap().clear();
    reload(cx);
    assert!(ASKED.lock().unwrap().is_empty());
}

#[gpui::test]
fn an_approval_for_another_fleet_admits_nothing(cx: &mut TestAppContext) {
    let _serial = serial();
    let cx = page(cx, true);
    JOINS.lock().unwrap().push(join("j1", "d1"));
    cx.update(|_, cx| Fleet::expect(cx, DeviceId("d1".into()), "e".repeat(64)));
    cx.run_until_parked();
    draw(cx);
    assert!(ASKED.lock().unwrap().is_empty());
    assert!(cx.debug_bounds("fleet-join-0").is_some());
}
