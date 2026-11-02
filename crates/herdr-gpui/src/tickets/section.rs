//! The Tickets section: its page across the window's body, or what shows
//! in its place while the page is created, when it cannot be, and in builds
//! without embedded pages.

use crate::{HerdrWindow, sidebar::ActivityPanel};
use gpui::{prelude::*, *};

impl HerdrWindow {
    /// Shows the Tickets section, or, while it shows, the terminals again
    /// with the keyboard, leaving the sidebar folded or open as it was.
    pub(crate) fn toggle_tickets(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.activity_section() != Some(ActivityPanel::Tickets) {
            self.show_activity_panel(ActivityPanel::Tickets, window, cx);
            return;
        }
        self.blur_tickets_page();
        if self.activity.pick(ActivityPanel::Workspaces) {
            self.save_chrome();
        }
        window.focus(&self.focus, cx);
        cx.notify();
    }

    pub(crate) fn render_tickets_section(&self, cx: &mut Context<Self>) -> AnyElement {
        #[cfg(any(target_os = "macos", windows))]
        if let Some(page) = self.tickets_page_element() {
            return div()
                .debug_selector(|| "tickets-section".into())
                .flex()
                .flex_1()
                .size_full()
                .min_w_0()
                .min_h_0()
                .child(page)
                .into_any_element();
        }
        let url = self.config.tickets.url();
        let message: SharedString = match self.tickets.failed() {
            Some(error) => format!("Cannot show the tickets board: {error}").into(),
            None if crate::browser::EMBEDDED => "Opening the tickets board…".into(),
            None => "This build cannot show pages inside the window.".into(),
        };
        let theme = &self.theme;
        let font = &self.config.ui;
        div()
            .debug_selector(|| "tickets-section".into())
            .flex()
            .flex_col()
            .flex_1()
            .size_full()
            .items_center()
            .justify_center()
            .gap_3()
            .p_5()
            .bg(rgb(theme.background))
            .text_color(rgb(theme.muted))
            .text_size(px(font.size))
            .child(
                div()
                    .debug_selector(|| "tickets-notice".into())
                    .child(message),
            )
            .when_some(url, |section, url| {
                section.child(
                    div()
                        .id("tickets-open-browser")
                        .debug_selector(|| "tickets-open-browser".into())
                        .px(px(10.))
                        .h(px(24.))
                        .flex()
                        .items_center()
                        .rounded(px(crate::config::corners::CONTROL))
                        .border_1()
                        .border_color(rgb(theme.active))
                        .text_color(rgb(theme.foreground))
                        .cursor_pointer()
                        .hover(|style| style.bg(rgb(theme.active)))
                        .child(format!("Open {} in the browser", url.address()))
                        .on_click(cx.listener(move |_, _, _, cx| cx.open_url(url.as_str()))),
                )
            })
            .into_any_element()
    }
}
