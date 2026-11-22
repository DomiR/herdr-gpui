use super::*;
use crate::sidebar::layout_tests::fixture_window;

/// A window a layout draws, with this machine and a saved Build device.
fn hosted(window: &mut Window, cx: &mut Context<SheprdWindow>) -> SheprdWindow {
    let mut view = fixture_window(window, cx);
    view.hosted = true;
    view.endpoints.push(crate::endpoint::Endpoint::new(
        "ssh:build".into(),
        "Build".into(),
        ConnectTarget::Ssh {
            target: "build.invalid".into(),
            session: "default".into(),
        },
        true,
    ));
    view.sessions.entries = Vec::new();
    view
}

fn kinds(picker: &Value) -> Vec<&str> {
    picker["sheet"]["lines"]
        .as_array()
        .unwrap()
        .iter()
        .map(|line| line["kind"].as_str().unwrap())
        .collect()
}

#[gpui::test]
fn the_footer_shows_while_the_sheet_waits_to_open(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(hosted);
    view.update_in(cx, |view, window, cx| {
        let picker = view.hosted_device_picker();
        assert_eq!(picker["footer"]["label"], "All Devices");
        assert_eq!(picker["open"], false);
        assert!(picker["sheet"].is_null());

        view.hosted_picker_toggle(window, cx);
        let picker = view.hosted_device_picker();
        assert_eq!(picker["open"], true);
        assert_eq!(kinds(&picker), ["all", "device", "device", "add"]);
        let lines = picker["sheet"]["lines"].as_array().unwrap();
        assert_eq!(lines[2]["label"], "Build");
        assert_eq!(lines[2]["key"], "device:ssh:build");
        assert_eq!(lines[2]["ordinal"], 2);
        assert_eq!(picker["sheet"]["selected"], 0);
        // The activity card leads it, with a bar slot for every step.
        assert!(
            !picker["sheet"]["activity"]["bars"]
                .as_array()
                .unwrap()
                .is_empty()
        );

        // The window's own picker is not drawn over the layout's.
        assert!(view.hosted_picker_open());
        view.hosted_picker_toggle(window, cx);
        assert_eq!(view.hosted_device_picker()["open"], false);
    });
}

#[gpui::test]
fn the_classic_window_draws_its_own_picker(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(hosted);
    view.update(cx, |view, _| {
        view.hosted = false;
        view.menu.page = Some(Page::Devices);
        assert!(!view.hosted_picker_open());
        assert!(view.hosted_device_picker()["sheet"].is_null());
    });
}

#[gpui::test]
fn a_line_acts_only_while_it_is_still_the_one_clicked(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(hosted);
    view.update_in(cx, |view, window, cx| {
        view.set_device_scope(DeviceScope::only(String::from("ssh:build")));
        view.hosted_picker_toggle(window, cx);

        // A click on a line that changed under it does nothing.
        view.hosted_picker_line(0, "device:ssh:build", LineAction::Choose, window, cx);
        assert_eq!(view.menu.page, Some(Page::Devices));
        assert_ne!(view.device_filter, DeviceScope::All);

        // The session chip unfolds this machine's sessions, and folds them.
        let local = format!("device:{}", view.endpoints[0].id);
        view.hosted_picker_line(1, &local, LineAction::Sessions, window, cx);
        assert_eq!(
            view.menu.unfolded_device.as_deref(),
            Some(view.endpoints[0].id.as_str())
        );
        view.hosted_picker_line(1, &local, LineAction::Sessions, window, cx);
        assert_eq!(view.menu.unfolded_device, None);

        // All Devices, clicked, lists every device and closes the sheet.
        view.hosted_picker_line(0, "all", LineAction::Choose, window, cx);
        assert_eq!(view.device_filter, DeviceScope::All);
        assert_eq!(view.menu.page, None);
    });
}

#[gpui::test]
fn the_keys_move_through_the_lines_and_escape_closes(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(hosted);
    view.update_in(cx, |view, window, cx| {
        view.hosted_picker_toggle(window, cx);
        view.hosted_picker_key("down", window, cx);
        assert_eq!(view.hosted_device_picker()["sheet"]["selected"], 1);
        view.hosted_picker_key("up", window, cx);
        view.hosted_picker_key("up", window, cx);
        // Up from the first wraps to Add Device.
        assert_eq!(view.hosted_device_picker()["sheet"]["selected"], 3);
        view.hosted_picker_key("escape", window, cx);
        assert_eq!(view.menu.page, None);
    });
}

#[gpui::test]
fn a_secondary_click_lists_a_device_too_and_keeps_the_sheet(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(hosted);
    view.update_in(cx, |view, window, cx| view.hosted_picker_toggle(window, cx));
    let secondary = cx.update(|_, cx| gpui::Modifiers::secondary_key(cx));
    cx.simulate_modifiers_change(secondary);
    view.update_in(cx, |view, window, cx| {
        // From All Devices, the device on screen and the one clicked: with
        // only two, this machine alone.
        let local = view.endpoints[0].id.clone();
        view.hosted_picker_line(
            1,
            &format!("device:{local}"),
            LineAction::Choose,
            window,
            cx,
        );
        assert_eq!(view.device_filter, DeviceScope::only(local));
        assert_eq!(view.menu.page, Some(Page::Devices));
    });
}
