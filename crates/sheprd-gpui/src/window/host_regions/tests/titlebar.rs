use super::*;
use gpui::{Entity, Render};

/// Draws the main window's `titlebar` region, as a host's view does.
struct TitleBarRegion {
    view: Entity<SheprdWindow>,
}

impl Render for TitleBarRegion {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.view.update(cx, |view, cx| {
            view.render_region(Region::TitleBar, window, cx)
        })
    }
}

#[gpui::test]
fn the_title_bar_region_ends_with_the_details_toggle_and_reports_its_width(
    cx: &mut TestAppContext,
) {
    let (region, cx) = cx.add_window_view(|window, cx| {
        let view = cx.new(|cx| {
            let mut view = crate::sidebar::layout_tests::fixture_window(window, cx);
            view.hosted = true;
            view
        });
        TitleBarRegion { view }
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    // Measured as it was drawn, so the strip can leave it free.
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let view = region.read_with(cx, |region, _| region.view.clone());
    assert!(view.read_with(cx, |view, _| view.titlebar_width) > 0.);

    let toggle = cx.debug_bounds("toggle-details").unwrap();
    cx.simulate_click(toggle.center(), Default::default());
    let shown = view.update(cx, |view, cx| {
        (
            view.details_visible,
            view.hosted_state(cx).main["details_visible"].clone(),
        )
    });
    assert_eq!(shown, (true, serde_json::Value::Bool(true)));
}
