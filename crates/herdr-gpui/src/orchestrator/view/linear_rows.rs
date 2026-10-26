//! The Linear section's list drawn as Linear's own: a filled heading per
//! assignee with their avatar, a heading per workflow state with a rule to
//! the edge, and rows of priority, identifier, state, title, and the parent
//! a sub-issue belongs to, with chips on the right for linked pull
//! requests, the project, and the estimate. Grouping is `linear_groups.rs`.

use super::{
    Event, Look, OrchestratorView,
    board::status_of,
    conversation::initials,
    linear_groups::{self, Group},
    linear_parts,
    list::blocked,
    look::YELLOW,
    rows::ItemRow,
    worktree::ROW_GROUP,
};
use crate::{config::corners, orchestrator::Item};
use gpui::{prelude::*, *};
use std::collections::BTreeSet;

/// The identifier column at the usual text size, wide enough for
/// `CORE-1234`.
const IDENTIFIER_WIDTH: f32 = 76.;
/// The estimate column at the usual text size.
const ESTIMATE_WIDTH: f32 = 30.;
const AVATAR: f32 = 18.;

impl OrchestratorView {
    /// Reads the pictures of the listed Linear issues' assignees, as a
    /// listing arrives: never while drawing.
    pub(super) fn request_avatars(&self, cx: &mut Context<Self>) {
        let urls: BTreeSet<&str> = self
            .repos
            .iter()
            .flat_map(|repo| repo.snapshot.items.iter())
            .filter_map(|item| item.tracker.as_ref()?.avatar.as_deref())
            .collect();
        crate::markdown_view::request_images(urls, cx);
    }

    /// The heading of `group`, the issues assigned to one person: a filled
    /// bar a click folds or unfolds.
    pub(super) fn render_assignee(
        &self,
        index: usize,
        group: Group,
        count: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let look = &self.look;
        let theme = &look.theme;
        let item = self.row_item(&group.row);
        let name = item
            .tracker
            .as_ref()
            .and_then(|tracker| tracker.assignee.clone());
        let key = linear_groups::assignee_key(item);
        let selector = format!("orchestrator-{key}");
        let avatar = match &name {
            Some(name) => self.assignee_avatar(item, name, cx),
            // Nobody's picture, as Linear draws it: an empty dashed circle.
            None => div()
                .flex_none()
                .size(px(AVATAR))
                .rounded_full()
                .border_1()
                .border_dashed()
                .border_color(rgb(theme.muted))
                .into_any_element(),
        };
        let label = name.unwrap_or_else(|| "No assignee".into());
        div()
            .id(("orchestrator-assignee", index))
            .debug_selector(move || selector.clone())
            .aria_label(label.clone())
            .aria_expanded(group.open)
            .w_full()
            .h(px(self.row_height()))
            .px_2()
            .py(px(3.))
            .cursor_pointer()
            .on_click(cx.listener(move |this, _, _, cx| {
                this.toggle_children(key.clone(), cx);
            }))
            .child(
                div()
                    .size_full()
                    .px_2()
                    .flex()
                    .items_center()
                    .gap_2()
                    .rounded(px(corners::CONTROL))
                    .bg(rgb(theme.surface))
                    .child(chevron(look, group.open))
                    .child(avatar)
                    .child(
                        div()
                            .min_w_0()
                            .truncate()
                            .font_weight(FontWeight::MEDIUM)
                            .child(label),
                    )
                    .child(total(look, group.estimate, count)),
            )
            .into_any_element()
    }

    /// The heading of `group`, one assignee's issues in one workflow state,
    /// with a rule to the edge; a click folds or unfolds it.
    pub(super) fn render_state(
        &self,
        index: usize,
        group: Group,
        count: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let look = &self.look;
        let theme = &look.theme;
        let item = self.row_item(&group.row);
        let status = status_of(item);
        let key = linear_groups::state_key(item);
        let selector = format!("orchestrator-{key}");
        let label = status.map_or_else(|| "No status".to_owned(), |status| status.name.clone());
        div()
            .id(("orchestrator-state", index))
            .debug_selector(move || selector.clone())
            .aria_label(label.clone())
            .aria_expanded(group.open)
            .w_full()
            .h(px(self.row_height()))
            .px_4()
            .flex()
            .items_center()
            .gap_2()
            .cursor_pointer()
            .on_click(cx.listener(move |this, _, _, cx| {
                this.toggle_children(key.clone(), cx);
            }))
            .child(chevron(look, group.open))
            .child(linear_parts::state_icon(look, status))
            .child(
                div()
                    .flex_none()
                    .font_weight(FontWeight::MEDIUM)
                    .child(label),
            )
            .child(total(look, group.estimate, count))
            .child(div().flex_1().h(px(1.)).bg(rgb(theme.active)))
            // A group's "+" starts a new issue in its state.
            .children(
                status
                    .and_then(|status| status.id.clone())
                    .map(|state| self.new_issue_plus("orchestrator-state", index, Some(state), cx)),
            )
            .into_any_element()
    }

    /// A Linear issue's row: priority, identifier, state, title and parent,
    /// then its chips. Start agent and Go to worktree show while the
    /// pointer is on it.
    pub(super) fn render_linear_row(
        &self,
        index: usize,
        row: ItemRow,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let look = &self.look;
        let theme = &look.theme;
        let compact = self.compact();
        let item = self.row_item(&row);
        let tracker = item.tracker.as_ref();
        let key = item.key.canonical();
        let selector = format!("orchestrator-row-{key}");
        let parent = tracker
            .and_then(|tracker| tracker.parent.as_deref())
            .filter(|_| !compact);
        let start = self.start_button(index, &row, cx);
        let worktree = self.worktree_button(&row, ROW_GROUP, cx);
        let actions = (start.is_some() || worktree.is_some()).then(|| {
            div()
                .absolute()
                .top_0()
                .bottom_0()
                .right(px(12.))
                .pl_2()
                .flex()
                .items_center()
                .gap_1()
                .bg(rgb(theme.surface))
                .invisible()
                .group_hover(ROW_GROUP, |style| style.visible())
                .children(start)
                .children(worktree)
        });
        let estimate = tracker.and_then(|tracker| tracker.estimate);
        self.row_shell(("orchestrator-issue", index), item, cx)
            .debug_selector(move || selector.clone())
            .group(ROW_GROUP)
            .relative()
            .border_b_0()
            .gap_3()
            .child(linear_parts::priority_mark(look, item.priority))
            .child(
                look.mono(item.identifier.clone())
                    .flex_none()
                    .w(px(look.row(IDENTIFIER_WIDTH)))
                    .truncate()
                    .text_color(rgb(theme.muted)),
            )
            .children(status_of(item).map(|status| linear_parts::state_icon(look, Some(status))))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(div().min_w_0().truncate().child(item.title.clone()))
                    .children(parent.map(|parent| {
                        div()
                            .min_w_0()
                            .truncate()
                            .text_color(rgb(theme.muted))
                            .child(format!("\u{203a} {parent}"))
                    }))
                    .when(blocked(item), |el| {
                        el.child(look.icon("icons/lock.svg", 11., look.hue(YELLOW)))
                    }),
            )
            .when(row.run.is_some(), |el| el.child(self.run_mark(&row)))
            .children(
                tracker
                    .and_then(|tracker| {
                        linear_parts::pull_request(
                            look,
                            &tracker.pull_requests,
                            ("orchestrator-pr-chip", index),
                            format!("orchestrator-pr-chip-{key}"),
                        )
                    })
                    .map(|(chip, url)| {
                        chip.on_click(cx.listener(move |_, _, _, cx| {
                            cx.emit(Event::OpenUrl(url.clone()));
                        }))
                    }),
            )
            .when(!compact, |el| {
                el.children(
                    tracker
                        .and_then(|tracker| tracker.project.as_ref())
                        .map(|project| linear_parts::project(look, project)),
                )
            })
            .child(
                div()
                    .flex_none()
                    .w(px(look.row(ESTIMATE_WIDTH)))
                    .flex()
                    .justify_end()
                    .children(estimate.map(|estimate| linear_parts::estimate(look, estimate))),
            )
            .children(actions)
            .into_any_element()
    }

    /// The assignee's picture once read, else their initial.
    fn assignee_avatar(&self, item: &Item, name: &str, cx: &App) -> AnyElement {
        let image = item
            .tracker
            .as_ref()
            .and_then(|tracker| tracker.avatar.as_deref())
            .and_then(|url| crate::markdown_view::remote_image(url, cx));
        match image {
            Some(image) => img(image)
                .flex_none()
                .size(px(AVATAR))
                .rounded_full()
                .into_any_element(),
            None => initials(&self.look, name, AVATAR),
        }
    }
}

/// A group's fold mark.
fn chevron(look: &Look, open: bool) -> AnyElement {
    let icon = if open {
        "icons/chevron-down.svg"
    } else {
        "icons/chevron-right.svg"
    };
    look.icon(icon, 10., look.theme.muted)
}

/// A heading's sum of estimates, else how many issues it holds.
fn total(look: &Look, estimate: Option<u32>, count: usize) -> Div {
    match estimate {
        Some(estimate) => linear_parts::estimate(look, estimate),
        None => look.muted(count.to_string()).flex_none(),
    }
}
