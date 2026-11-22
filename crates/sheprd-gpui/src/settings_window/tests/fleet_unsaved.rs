//! Settings > Fleet says when fleet hosts could not be added to Devices,
//! as when the saved devices are full.
use super::fleet::{page, serial};
use super::*;
use crate::fleet::{Fleet, saved::Unsaved};

#[gpui::test]
fn fleet_hosts_that_could_not_be_saved_are_shown(cx: &mut TestAppContext) {
    let _serial = serial();
    let cx = page(cx, true);
    assert!(cx.debug_bounds("fleet-unsaved").is_none());
    cx.update(|_, cx| {
        Fleet::set_unsaved(
            cx,
            Some(Unsaved {
                count: 3,
                reason: "too many saved devices".into(),
            }),
        );
    });
    cx.run_until_parked();
    remote_control::draw(cx);
    assert!(cx.debug_bounds("fleet-unsaved").is_some());
}
