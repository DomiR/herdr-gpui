//! Hide statuses, in the Linear section's header: a menu of every workflow
//! state the listed issues are in or their teams have, each a check that
//! hides the issues in it, rows from the list and lanes from the board. The
//! states hidden are the view's, by name, for as long as the window is open;
//! only Linear's issues have states, so they narrow nothing else.

use super::{IssueSource, OrchestratorView, rows::Tab};
use crate::{
    config::corners,
    orchestrator::{Provider, Status, StatusKind},
};
use gpui::{prelude::*, *};

const MENU_WIDTH: f32 = 220.;
/// Taller menus scroll.
const MENU_HEIGHT: f32 = 360.;

impl OrchestratorView {
    /// Whether hidden statuses narrow what is shown.
    pub(super) fn hiding(&self) -> bool {
        self.issue_source == IssueSource::Linear && !self.hidden_statuses.is_empty()
    }

    /// Each workflow state the listed Linear issues are in or their teams
    /// have, once by name, in board order. Canceled shows only while an
    /// issue is in it, as the board leaves it off.
    fn known_statuses(&self) -> Vec<&Status> {
        let mut known: Vec<&Status> = Vec::new();
        let trackers = self
            .repos
            .iter()
            .flat_map(|repo| repo.snapshot.items.iter())
            .filter(|item| item.key.source.provider == Provider::Linear)
            .filter_map(|item| item.tracker.as_ref());
        for tracker in trackers {
            let states = tracker
                .states
                .iter()
                .filter(|state| state.kind != StatusKind::Canceled);
            for status in tracker.status.iter().chain(states) {
                if !known.iter().any(|seen| seen.name == status.name) {
                    known.push(status);
                }
            }
        }
        known.sort_by(|a, b| a.order().cmp(&b.order()));
        known
    }

    /// Hides the issues in the state named `name`, or shows them again.
    fn toggle_hidden(&mut self, name: String, cx: &mut Context<Self>) {
        if !self.hidden_statuses.remove(&name) {
            self.hidden_statuses.insert(name);
        }
        self.refresh_rows();
        cx.notify();
    }

    /// Hide statuses, with how many are hidden, and its menu under it
    /// while open, in the Linear section's header.
    pub(super) fn render_hide_statuses(&self, cx: &mut Context<Self>) -> Option<Div> {
        if self.tab != Tab::Issues || self.issue_source != IssueSource::Linear {
            return None;
        }
        let look = &self.look;
        let theme = &look.theme;
        let hidden = self.hidden_statuses.len();
        let open = self.hide_open;
        let button = div()
            .id("orchestrator-hide-statuses")
            .debug_selector(|| "orchestrator-hide-statuses".into())
            .role(Role::Button)
            .aria_label("Hide statuses")
            .flex_none()
            .flex()
            .items_center()
            .gap_1()
            .px_2()
            .py(px(3.))
            .rounded(px(corners::CONTROL))
            .border_1()
            .border_color(rgb(if open || hidden > 0 {
                theme.primary()
            } else {
                theme.active
            }))
            .when(open, |el| el.bg(rgb(theme.primary_wash())))
            .cursor_pointer()
            .text_size(look.small())
            .child("Hide statuses")
            .when(hidden > 0, |el| {
                el.child(
                    div()
                        .debug_selector(|| "orchestrator-hidden-count".into())
                        .px(px(5.))
                        .rounded_full()
                        .bg(rgb(theme.primary()))
                        .text_color(rgb(theme.text_on(theme.primary())))
                        .text_size(look.tiny())
                        .child(hidden.to_string()),
                )
            })
            .child(look.icon("icons/chevron-down.svg", 10., theme.muted))
            .on_click(cx.listener(|this, _, _, cx| {
                this.hide_open = !this.hide_open;
                cx.notify();
            }));
        Some(div().relative().flex_none().child(button).when(open, |el| {
            // Hung from the button's bottom right corner.
            el.child(
                div().absolute().top_full().right_0().child(
                    deferred(
                        anchored()
                            .anchor(Anchor::TopRight)
                            .snap_to_window()
                            .child(self.render_hide_menu(cx)),
                    )
                    .with_priority(1),
                ),
            )
        }))
    }

    /// A check per known state, checked while hidden; a state hidden but
    /// no longer known stays, so it can be shown again.
    fn render_hide_menu(&self, cx: &mut Context<Self>) -> Stateful<Div> {
        let look = &self.look;
        let theme = &look.theme;
        let mut entries: Vec<(String, u32)> = self
            .known_statuses()
            .into_iter()
            .map(|status| (status.name.clone(), self.status_hue(status)))
            .collect();
        for name in &self.hidden_statuses {
            if !entries.iter().any(|(known, _)| known == name) {
                entries.push((name.clone(), theme.muted));
            }
        }
        let hover = theme.active;
        let empty = entries.is_empty();
        div()
            .id("orchestrator-hide-menu")
            .debug_selector(|| "orchestrator-hide-menu".into())
            .mt_1()
            .w(px(MENU_WIDTH))
            .max_h(px(MENU_HEIGHT))
            .overflow_y_scroll()
            .p_1()
            .flex()
            .flex_col()
            .rounded(px(corners::CONTROL))
            .bg(rgb(theme.surface))
            .border_1()
            .border_color(rgb(theme.active))
            .shadow_lg()
            .text_size(look.small())
            .when(empty, |el| {
                el.child(look.muted("No statuses listed yet").px_2().py_1())
            })
            .children(entries.into_iter().map(|(name, hue)| {
                let on = self.hidden_statuses.contains(&name);
                let id = format!("orchestrator-hide-status-{name}");
                let selector = id.clone();
                div()
                    .id(SharedString::from(id))
                    .debug_selector(move || selector.clone())
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap_2()
                    .px_2()
                    .h(px(28.))
                    .rounded(px(corners::SMALL))
                    .cursor_pointer()
                    .hover(move |style| style.bg(rgb(hover)))
                    .child(
                        div()
                            .flex_none()
                            .size(px(11.))
                            .rounded(px(2.))
                            .border_1()
                            .border_color(rgb(if on { theme.primary() } else { theme.muted }))
                            .when(on, |el| el.bg(rgb(theme.primary()))),
                    )
                    .child(look.dot(hue, 8.))
                    .child(div().flex_1().min_w_0().truncate().child(name.clone()))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.toggle_hidden(name.clone(), cx);
                    }))
            }))
    }
}
