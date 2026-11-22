//! Settings > Fleet, plain hosts: a machine with no Sheprd daemon offers
//! "Share as a plain host", which says the relay agent is installed and
//! installs nothing until a person confirms; a plain host already in the
//! fleet shows as one.
use super::fleet::{ASKED, page, serial};
use super::remote_control::{click, draw};
use super::*;
use crate::{
    device_tags::Daemon,
    fleet::{Fleet, Standing},
};
use sheprd_client::ConnectTarget;
use std::collections::BTreeMap;

/// A window that reaches one SSH machine, `build-01`, which said
/// `standing` when last asked, so the page does not sync it again.
fn reach(cx: &mut VisualTestContext, standing: Standing) {
    cx.update(|_, cx| {
        Fleet::set_standing(cx, "build-01", standing);
        let window = cx.new(|_| ()).entity_id();
        let reachable = BTreeMap::from([(
            "build-01".to_owned(),
            Daemon::Remote(ConnectTarget::Ssh {
                target: "deploy@build-01".into(),
                session: "default".into(),
            }),
        )]);
        Fleet::reach(cx, window, &reachable);
    });
    cx.run_until_parked();
    draw(cx);
}

#[gpui::test]
fn a_machine_without_sheprd_is_shared_as_a_plain_host_only_once_confirmed(cx: &mut TestAppContext) {
    let _serial = serial();
    let cx = page(cx, true);
    reach(cx, Standing::NoDaemon);
    // Invite and Share need a Sheprd daemon there.
    assert!(cx.debug_bounds("fleet-machine-0-invite").is_none());
    assert!(cx.debug_bounds("fleet-machine-0-share").is_none());
    click(cx, "fleet-machine-0-share-plain");
    cx.run_until_parked();
    draw(cx);
    // The first click only asks: nothing is installed or signed, and the
    // computers to allow are listed.
    assert!(ASKED.lock().unwrap().is_empty());
    assert!(!cx.update(|_, cx| Fleet::acting(cx, "build-01")));
    assert!(cx.debug_bounds("fleet-machine-0-share-0-toggle").is_some());
    click(cx, "fleet-machine-0-share-1-toggle");
    click(cx, "fleet-machine-0-share-confirm");
    cx.run_until_parked();
    draw(cx);
    // This computer listens on no relay in the stand-in fleet, so the
    // share stops before reaching the machine, and says why.
    assert!(cx.debug_bounds("fleet-error").is_some());
    assert!(ASKED.lock().unwrap().is_empty());
}

#[gpui::test]
fn a_plain_host_in_the_fleet_shows_as_one(cx: &mut TestAppContext) {
    let _serial = serial();
    let cx = page(cx, true);
    reach(cx, Standing::Agent);
    assert!(cx.debug_bounds("fleet-machine-0-share-plain").is_none());
    assert!(cx.debug_bounds("fleet-machine-0-invite").is_none());
    assert!(cx.debug_bounds("fleet-machine-0-scrub").is_some());
    assert_eq!(
        cx.update(|_, cx| Fleet::standing(cx, "build-01")),
        Standing::Agent
    );
}

#[gpui::test]
fn a_machine_whose_key_is_not_checked_cannot_be_shared(cx: &mut TestAppContext) {
    let _serial = serial();
    let cx = page(cx, true);
    reach(cx, Standing::Unchecked);
    assert!(cx.debug_bounds("fleet-machine-0-share-plain").is_none());
    assert!(cx.debug_bounds("fleet-machine-0-share").is_none());
    assert!(cx.debug_bounds("fleet-machine-0-invite").is_none());
    assert!(cx.debug_bounds("fleet-machine-0-scrub").is_some());
}

#[gpui::test]
fn sharing_a_machine_whose_key_is_not_checked_stops_before_reaching_it(cx: &mut TestAppContext) {
    let _serial = serial();
    let cx = page(cx, true);
    reach(cx, Standing::NoDaemon);
    sheprd_client::set_host_keys("deploy@build-01", sheprd_client::HostKeys::Skip);
    click(cx, "fleet-machine-0-share-plain");
    click(cx, "fleet-machine-0-share-confirm");
    cx.run_until_parked();
    draw(cx);
    sheprd_client::set_host_keys("deploy@build-01", sheprd_client::HostKeys::Verify);
    assert!(cx.debug_bounds("fleet-error").is_some());
    assert!(!cx.update(|_, cx| Fleet::acting(cx, "build-01")));
    assert!(ASKED.lock().unwrap().is_empty());
}
