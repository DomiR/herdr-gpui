use super::*;

fn drag(
    view: &mut SheprdWindow,
    phase: Drag,
    dx: f32,
    window: &mut gpui::Window,
    cx: &mut gpui::Context<SheprdWindow>,
) {
    view.hosted_action(Action::SidebarWidth { phase, dx }, window, cx);
}

#[gpui::test]
fn dragging_the_collapsed_edge_out_opens_the_sidebar(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(crate::sidebar::layout_tests::fixture_window);
    cx.simulate_resize(gpui::size(gpui::px(1200.), gpui::px(800.)));
    view.update_in(cx, |view, window, cx| {
        view.sidebar_visible = false;
        drag(view, Drag::Down { reset: false }, 0., window, cx);

        // Short of the least width, it stays collapsed.
        drag(view, Drag::Move, 20., window, cx);
        assert!(!view.sidebar_visible);

        // Past it, the sidebar opens at the dragged width.
        drag(view, Drag::Move, 260., window, cx);
        drag(view, Drag::Up, 260., window, cx);
        assert!(view.sidebar_visible);
        let width = view.sidebar_width.unwrap();
        assert!(width >= crate::sidebar::metrics::SIDEBAR_LEAST, "{width}");
    });
}

#[gpui::test]
fn back_and_forward_leave_the_header_while_the_sidebar_is_folded(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(crate::sidebar::layout_tests::fixture_window);
    cx.simulate_resize(gpui::size(gpui::px(1200.), gpui::px(800.)));
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let shown = |view: &gpui::Entity<SheprdWindow>, cx: &mut gpui::VisualTestContext| {
        view.update(cx, |view, cx| {
            view.hosted_state(cx).main["navigation_in_header"].clone()
        })
    };
    assert_eq!(shown(&view, cx), Value::Bool(true));
    view.update(cx, |view, _| view.sidebar_visible = false);
    assert_eq!(shown(&view, cx), Value::Bool(false));
}
