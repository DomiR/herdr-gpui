use super::*;

#[gpui::test]
fn rows_under_a_device_header_step_in_as_the_layout_says(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(crate::sidebar::layout_tests::fixture_window);
    view.update(cx, |view, cx| {
        for mode in crate::config::LayoutMode::ALL {
            view.config.layout.mode = mode;
            let expected = SidebarMetrics::for_mode(mode).nest;
            let layout = &view.hosted_state(cx).main["layout"];
            assert_eq!(
                layout["nest"].as_f64(),
                Some(f64::from(expected)),
                "{mode:?}"
            );
        }
        // A configured `indent` moves them as it moves the window's own rows.
        view.config.sidebar_style.overrides.indent = Some(30.);
        let layout = &view.hosted_state(cx).main["layout"];
        assert_eq!(layout["nest"].as_f64(), Some(30.));
    });
}
