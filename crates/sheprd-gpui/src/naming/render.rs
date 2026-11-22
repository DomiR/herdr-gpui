//! The row under a rename dialog's field that asks for a name: Claude on
//! its own, and the agent working there when the subject has one.

use super::Way;
use crate::{SheprdWindow, i18n::fl, menu::danger};
use gpui::{prelude::*, *};

impl SheprdWindow {
    /// The naming row of the open rename dialog, when it offers naming.
    pub(crate) fn render_naming(&self, cx: &mut Context<Self>) -> Option<Div> {
        let naming = self.menu.naming.as_ref()?;
        let theme = &self.theme;
        let running = naming.running();
        let button = |id: &'static str, icon: &'static str, label: String, way: Way| {
            let busy = running.is_some();
            div()
                .id(id)
                .debug_selector(move || id.into())
                .flex()
                .flex_none()
                .items_center()
                .gap(px(6.))
                .px(px(8.))
                .py(px(4.))
                .rounded(px(crate::config::corners::CONTROL))
                .border_1()
                .border_color(rgb(theme.active))
                .text_color(rgb(theme.subtext()))
                .child(
                    svg()
                        .path(icon)
                        .size(px(14.))
                        .flex_none()
                        .text_color(rgb(theme.muted)),
                )
                .child(if running == Some(way) {
                    fl!("naming-running")
                } else {
                    label
                })
                .when(busy, |button| {
                    button.opacity(0.6).cursor(CursorStyle::OperationNotAllowed)
                })
                .when(!busy, |button| {
                    button
                        .cursor_pointer()
                        .hover(|button| button.bg(rgb(theme.active)))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            cx.stop_propagation();
                            this.start_naming(way, cx);
                        }))
                })
        };
        let row = div()
            .flex()
            .flex_wrap()
            .gap(px(8.))
            .child(button(
                "naming-claude",
                "icons/sparkles.svg",
                fl!("naming-with-claude"),
                Way::Claude,
            ))
            .children(naming.agent().map(|agent| {
                button(
                    "naming-agent",
                    "icons/chat.svg",
                    fl!("naming-ask-agent", agent = agent),
                    Way::Agent,
                )
            }));
        Some(
            div()
                .flex()
                .flex_col()
                .gap(px(6.))
                .child(row)
                .children(naming.error().map(|error| {
                    div()
                        .debug_selector(|| "naming-error".into())
                        // No width of its own, so the dialog sets it and the
                        // message wraps inside it.
                        .w_0()
                        .min_w_full()
                        .text_color(danger(theme))
                        .child(error.to_owned())
                })),
        )
    }
}
