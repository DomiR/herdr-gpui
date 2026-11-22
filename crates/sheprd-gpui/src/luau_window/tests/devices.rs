//! The device picker's messages: what the `devices` mod sends Sheprd.
use super::super::{Action, Data, action};
use crate::menu::LineAction;

#[test]
fn a_picker_line_names_its_place_key_and_what_was_done() {
    let line = |ordinal: f64, how: &str| {
        action(&Data::map([
            ("device_line", ordinal.into()),
            ("key", "device:ssh:build".into()),
            ("how", how.into()),
        ]))
    };
    assert_eq!(
        line(2.0, "sessions"),
        Some(Action::DeviceLine {
            ordinal: 2,
            key: "device:ssh:build".into(),
            action: LineAction::Sessions,
        })
    );
    for (how, action) in [
        ("choose", LineAction::Choose),
        ("delete", LineAction::Delete),
    ] {
        assert!(
            matches!(line(0.0, how), Some(Action::DeviceLine { action: sent, .. }) if sent == action)
        );
    }
    // Neither a place that is no line's nor a gesture it does not know.
    assert_eq!(line(-1.0, "choose"), None);
    assert_eq!(line(1.5, "choose"), None);
    assert_eq!(line(1.0, "drag"), None);
}

#[test]
fn the_footer_and_sheet_open_close_and_take_keys() {
    let flag = |key: &'static str| action(&Data::map([(key, true.into())]));
    assert_eq!(flag("device_picker"), Some(Action::DevicePicker));
    assert_eq!(
        flag("device_picker_dismiss"),
        Some(Action::DevicePickerDismiss)
    );
    assert_eq!(flag("devices_overview"), Some(Action::DevicesOverview));
    assert_eq!(
        action(&Data::map([("device_key", "down".into())])),
        Some(Action::DevicePickerKey("down".into()))
    );
}
