use super::*;
use crate::config::DeviceFooter;

#[gpui::test]
fn the_device_picker_region_follows_the_footer_setting(cx: &mut TestAppContext) {
    let (view, cx) = hosted(cx);
    let devices = |view: &gpui::Entity<SheprdWindow>, cx: &mut VisualTestContext| {
        cx.update(|window, cx| view.read(cx).region_sizes(window).devices)
    };
    assert_eq!(devices(&view, cx), crate::sidebar::DEVICE_FOOTER_HEIGHT);

    // The fixture has one device, so auto hides the picker as never does.
    for mode in [DeviceFooter::Auto, DeviceFooter::Never] {
        view.update(cx, |view, _| view.config.sidebar_style.device_footer = mode);
        assert_eq!(devices(&view, cx), 0., "{mode:?}");
    }
}

#[gpui::test]
fn sizes_carry_the_window_height_the_left_pane_shares(cx: &mut TestAppContext) {
    let (view, cx) = hosted(cx);
    cx.simulate_resize(gpui::size(px(800.), px(600.)));
    let height = cx.update(|window, cx| view.read(cx).region_sizes(window).height);
    assert_eq!(height, 600.);
}
