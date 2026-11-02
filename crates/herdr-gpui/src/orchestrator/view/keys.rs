//! The section's keys: the open dialog's, the composer's, then moving
//! through the list and opening what is selected.

use super::*;

impl OrchestratorView {
    pub(super) fn on_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if (self.dialog.is_some() || self.confirm.is_some() || self.linear.is_some())
            && self.dialog_key(&event.keystroke.key, window, cx)
        {
            cx.stop_propagation();
            return;
        }
        if self.dialog.is_some() || self.linear.is_some() {
            return;
        }
        if self.creating() {
            if self.create_key(event, window, cx) {
                cx.stop_propagation();
            }
            return;
        }
        if self.comment.read(cx).focus.is_focused(window) {
            match event.keystroke.key.as_str() {
                "enter" => self.post_comment(cx),
                "escape" => window.focus(&self.focus, cx),
                _ => return,
            }
            cx.stop_propagation();
            return;
        }
        if self.message.read(cx).focus.is_focused(window) {
            match event.keystroke.key.as_str() {
                "enter" => self.send_message(cx),
                "escape" => window.focus(&self.focus, cx),
                _ => return,
            }
            cx.stop_propagation();
            return;
        }
        if self.picker_key(event, window, cx) {
            return;
        }
        if self.search.read(cx).focus.is_focused(window) {
            if event.keystroke.key == "escape" {
                window.focus(&self.focus, cx);
                cx.stop_propagation();
            }
            return;
        }
        let modifiers = &event.keystroke.modifiers;
        if modifiers.platform || modifiers.control || modifiers.alt {
            return;
        }
        let handled = match (event.keystroke.key.as_str(), &self.detail) {
            ("escape", Some(_)) => {
                self.detail = None;
                cx.notify();
                true
            }
            ("escape", None) if self.hide_open => {
                self.hide_open = false;
                cx.notify();
                true
            }
            ("escape", None) if self.sort_open => {
                self.sort_open = false;
                cx.notify();
                true
            }
            ("/", None) => {
                window.focus(&self.search.read(cx).focus.clone(), cx);
                true
            }
            ("up" | "k", None) => {
                self.move_selection(-1, cx);
                true
            }
            ("down" | "j", None) => {
                self.move_selection(1, cx);
                true
            }
            ("enter", None) => {
                if let Some(key) = self.selected.clone() {
                    self.open_detail(key, cx);
                }
                true
            }
            ("c", _) if self.issue_source == IssueSource::Linear => {
                self.open_linear_create(None, window, cx);
                true
            }
            ("r", _) => {
                self.ask_refresh();
                true
            }
            ("1" | "2" | "3" | "4" | "5", Some(detail)) => {
                let index = event.keystroke.key.parse::<usize>().unwrap_or(1) - 1;
                let pull_request = self
                    .item(&detail.key)
                    .is_some_and(|item| item.github_pull_request().is_some());
                let tabs = DetailTab::tabs(pull_request);
                let mut detail = detail.clone();
                detail.tab = tabs[index.min(tabs.len() - 1)];
                self.detail = Some(detail);
                self.follow_pull_request();
                cx.notify();
                true
            }
            _ => false,
        };
        if handled {
            cx.stop_propagation();
        }
    }
}
