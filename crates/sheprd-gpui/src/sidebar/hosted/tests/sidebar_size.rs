use super::*;

#[test]
fn the_sidebar_size_is_the_step_nearest_its_scale_of_the_ui_size() {
    assert_eq!(text_step(12., 12.), "normal");
    assert_eq!(text_step(14., 12.), "large");
    assert_eq!(text_step(10., 12.), "small");
    assert_eq!(text_step(18., 12.), "title");
    // Far past the scale, the largest step.
    assert_eq!(text_step(64., 6.), "hero");
}

#[gpui::test]
fn the_theme_carries_the_sidebar_size(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(crate::sidebar::layout_tests::fixture_window);
    view.update(cx, |view, cx| {
        view.config.ui.size = 12.;
        view.config.sidebar.size = 14.;
        assert_eq!(view.hosted_state(cx).theme["sidebar_size"], "large");
        view.config.sidebar.size = 12.;
        assert_eq!(view.hosted_state(cx).theme["sidebar_size"], "normal");
    });
}
