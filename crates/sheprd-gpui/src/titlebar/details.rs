//! The details pane's toggle, at the bar's end over the pane it opens. Only
//! a window another app hosts has the pane: Moonframe lays it out beside
//! the main view and reads [`SheprdWindow::details_visible`].

use super::{SIDEBAR_GLYPH_WIDTH, SIDEBAR_TOGGLE_SIZE};
use crate::{direction::Logical, i18n::fl, window::SheprdWindow};
use gpui::{prelude::*, *};

impl SheprdWindow {
    pub(crate) fn details_toggle(&self, cx: &mut Context<Self>) -> AnyElement {
        div()
            .id("toggle-details")
            .debug_selector(|| "toggle-details".into())
            .role(Role::Button)
            .aria_label(fl!("titlebar-details"))
            .aria_expanded(self.details_visible)
            .map(|control| self.chrome_stop(crate::window::chrome_focus::Region::TitleBar, control))
            .flex_none()
            .self_center()
            .size(px(SIDEBAR_TOGGLE_SIZE))
            .row()
            .items_center()
            .justify_center()
            .rounded(px(crate::config::corners::CONTROL))
            .cursor_pointer()
            .hover(|style| style.bg(rgb(self.theme.active)))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(cx.listener(|this, _, _, cx| {
                cx.stop_propagation();
                this.details_visible = !this.details_visible;
                cx.notify();
            }))
            .child(details_glyph(self.details_visible, &self.theme))
            .into_any_element()
    }
}

/// The sidebar toggle's glyph mirrored: the panel on the trailing side,
/// filled while the pane shows.
fn details_glyph(expanded: bool, theme: &crate::config::Theme) -> Div {
    div()
        .w(px(SIDEBAR_GLYPH_WIDTH))
        .h(px(14.))
        .flex_none()
        .border_1()
        .border_color(rgb(theme.foreground))
        .rounded(px(2.))
        .row()
        .justify_end()
        .child(
            div()
                .w(px(5.))
                .h_full()
                .border_s(px(1.))
                .border_color(rgb(theme.foreground))
                .when(expanded, |bar| bar.bg(rgb(theme.foreground))),
        )
}
