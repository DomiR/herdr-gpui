//! Settings > Fleet says it needs Remote control, and opens that page,
//! while the daemon that keeps the fleet is off or not answering; a
//! daemon still starting offers Create as before.
use super::fleet::{page, serial};
use super::remote_control::{click, draw};
use super::*;
use crate::{
    fleet::Fleet,
    remote::{Health, Supervisor},
};

fn with_health(cx: &mut VisualTestContext, health: Health) {
    cx.update(|_, cx| {
        Supervisor::set_health(cx, health, None);
        Fleet::refresh(cx);
    });
    cx.run_until_parked();
    draw(cx);
}

#[gpui::test]
fn with_remote_control_on_the_page_offers_to_create_a_fleet(cx: &mut TestAppContext) {
    let _serial = serial();
    let cx = page(cx, false);
    assert!(cx.debug_bounds("fleet-create").is_some());
    assert!(cx.debug_bounds("fleet-open-remote").is_none());
    // Switched on and not asked yet: still offered, not blamed on it.
    with_health(cx, Health::Unknown);
    assert!(cx.debug_bounds("fleet-create").is_some());
    assert!(cx.debug_bounds("fleet-open-remote").is_none());
}

#[gpui::test]
fn with_remote_control_off_the_page_points_to_it(cx: &mut TestAppContext) {
    let _serial = serial();
    let cx = page(cx, false);
    with_health(cx, Health::Off);
    assert!(cx.debug_bounds("fleet-create").is_none());
    assert!(cx.debug_bounds("fleet-open-remote").is_some());
    click(cx, "fleet-open-remote");
    assert!(cx.debug_bounds("remote-enabled-toggle").is_some());
    assert!(cx.debug_bounds("fleet-open-remote").is_none());
}

#[gpui::test]
fn with_the_daemon_not_answering_the_page_points_to_remote_control(cx: &mut TestAppContext) {
    let _serial = serial();
    let cx = page(cx, false);
    with_health(cx, Health::NotRunning("refused".into()));
    assert!(cx.debug_bounds("fleet-create").is_none());
    assert!(cx.debug_bounds("fleet-open-remote").is_some());
}

#[gpui::test]
fn a_fleet_already_read_stays_shown_while_the_daemon_is_down(cx: &mut TestAppContext) {
    let _serial = serial();
    let cx = page(cx, true);
    cx.update(|_, cx| Supervisor::set_health(cx, Health::NotRunning("refused".into()), None));
    draw(cx);
    assert!(cx.debug_bounds("fleet-member-0").is_some());
    assert!(cx.debug_bounds("fleet-open-remote").is_none());
}

#[gpui::test]
fn remote_control_points_to_fleet(cx: &mut TestAppContext) {
    let _serial = serial();
    let (view, cx, _) = remote_control::page(cx);
    cx.update(|_, cx| Fleet::install(cx));
    click(cx, "remote-fleet-open");
    assert_eq!(view.read_with(cx, |view, _| view.section), Section::Fleet);
}
