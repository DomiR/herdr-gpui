//! The conversation as the agent received it: with "As sent" on, each
//! prompt shows what the chat program sent ahead of the person's words
//! (Sheprd's note, and the conversation a newly chosen agent is handed),
//! dimmed and in the terminal face, so what the agent was told can be
//! checked. The agent's own system prompt never leaves the agent, so it is
//! not among it, and the banner says so.

use super::*;
use crate::notifications::safe_text;
use sheprd_chat_protocol::{Entry, EntryKind};

/// Characters of a note drawn; the copy button takes it whole.
const MAX_SHOWN: usize = 64 * 1024;
/// Lines of a note drawn, each its own element, so a note of blank lines
/// costs no more to draw than one of text.
const MAX_LINES: usize = 1000;

impl SheprdWindow {
    /// The header button that turns "As sent" on and off.
    pub(super) fn render_chat_sent_toggle(&self, chat: &ChatPane, cx: &mut Context<Self>) -> Div {
        let theme = &self.theme;
        let on = chat.as_sent;
        let color = if on { theme.primary() } else { theme.subtext() };
        let pane_id = chat.target.pane_id.clone();
        div().child(
            div()
                .id("chat-as-sent")
                .debug_selector(|| "chat-as-sent".into())
                .flex_none()
                .flex()
                .items_center()
                .gap(px(4.))
                .px(px(6.))
                .h(px(22.))
                .rounded(px(corners::CONTROL))
                .cursor_pointer()
                .text_color(rgb(color))
                .when(on, |button| button.bg(rgb(theme.primary_wash())))
                .hover(|button| button.bg(rgb(theme.active)))
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    cx.stop_propagation();
                    if let Some(chat) = this.chats.pane_mut(&pane_id) {
                        chat.toggle_as_sent();
                        cx.notify();
                    }
                }))
                .child(
                    svg()
                        .path("icons/code.svg")
                        .size(px(13.))
                        .text_color(rgb(color)),
                )
                .child(fl!("chat-sent-toggle")),
        )
    }

    /// The line under the header while "As sent" is on.
    pub(super) fn render_chat_sent_banner(&self, chat: &ChatPane) -> Option<Div> {
        chat.as_sent.then(|| {
            div()
                .debug_selector(|| "chat-as-sent-banner".into())
                .px(px(12.))
                .py(px(4.))
                .bg(rgb(self.theme.surface))
                .text_color(rgb(self.theme.subtext()))
                .child(fl!("chat-sent-banner"))
        })
    }

    /// `row`, a prompt's row in the list, after what was sent ahead of it
    /// while "As sent" is on.
    pub(super) fn with_chat_sent_note(
        &self,
        chat: &ChatPane,
        entry: &Entry,
        row: AnyElement,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let EntryKind::User {
            note: Some(note), ..
        } = &entry.kind
        else {
            return row;
        };
        if !chat.as_sent {
            return row;
        }
        let theme = &self.theme;
        let id = entry.id.0;
        let copy = self
            .chat_copy_button(
                ("chat-sent-copy", id),
                format!("chat-sent-copy-{id}"),
                note.clone(),
                fl!("chat-copy-message-done"),
                cx,
            )
            .flex_none()
            .flex()
            .items_center()
            .gap(px(4.))
            .px(px(4.))
            .rounded(px(corners::SMALL))
            .text_color(rgb(theme.muted))
            .child(
                svg()
                    .path("icons/copy.svg")
                    .size(px(11.))
                    .text_color(rgb(theme.muted)),
            )
            .child(fl!("chat-copy"));
        let block = div()
            .debug_selector(move || format!("chat-sent-note-{id}"))
            .mx(px(12.))
            .mt(px(8.))
            .pl(px(8.))
            .flex()
            .flex_col()
            .gap(px(2.))
            .border_l_2()
            .border_color(rgb(Hues::of(theme).purple))
            .text_font(&self.config.terminal)
            .text_size(self.chat_small_text())
            .text_color(rgb(theme.muted))
            .child(div().flex().justify_end().child(copy))
            .child(div().min_w_0().flex().flex_col().children(lines(note)));
        div()
            .flex()
            .flex_col()
            .child(block)
            .child(row)
            .into_any_element()
    }
}

/// The note's lines with what untrusted text may not hold taken out, up to
/// [`MAX_LINES`] lines and [`MAX_SHOWN`] characters in all, then "…" when
/// more was left; a blank line keeps its height.
fn lines(note: &str) -> Vec<String> {
    let mut left = MAX_SHOWN;
    let mut shown = Vec::new();
    for line in note.lines() {
        if left == 0 || shown.len() == MAX_LINES {
            shown.push("…".to_owned());
            break;
        }
        let line = safe_text(line, left);
        left = left.saturating_sub(line.chars().count().max(1));
        shown.push(if line.is_empty() {
            " ".to_owned()
        } else {
            line
        });
    }
    shown
}

#[cfg(test)]
mod tests;
