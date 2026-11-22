//! A Bluetooth pairing's fleet failure stays shown until a later pairing's
//! fleet step succeeds, whatever the daemon's reads say meanwhile.

use super::*;

#[gpui::test]
fn a_pairing_failure_outlives_a_good_read(cx: &mut gpui::TestAppContext) {
    cx.update(|cx| {
        Fleet::install(cx);
        Fleet::pairing_done(cx, Some("could not reach studio".into()));
        cx.update_global::<Fleet, _>(|fleet, _| {
            fleet.reads = 1;
            fleet.answered(1, Ok(None));
        });
        assert_eq!(Fleet::error(cx).as_deref(), Some("could not reach studio"));
        Fleet::pairing_done(cx, None);
        assert_eq!(Fleet::error(cx), None);
    });
}
