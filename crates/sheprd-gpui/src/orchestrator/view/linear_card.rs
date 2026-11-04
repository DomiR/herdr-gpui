//! A Linear issue's card on the board, drawn as Linear's: a raised, rounded
//! surface with the identifier and the parent a sub-issue belongs to, the
//! state's mark beside a title of up to two lines, then outlined chips for
//! the priority, project, estimate, sub-issue progress, and linked pull
//! requests. It holds what it shows, so the card under the pointer while
//! dragged draws without the view.

use super::{Look, board::status_of, linear_parts};
use crate::{
    config::corners,
    orchestrator::{Item, LinkedPullRequest, Progress, Project, Status},
};
use gpui::{prelude::*, *};

/// What a card shows.
#[derive(Clone, Debug)]
pub(super) struct Card {
    /// The issue's canonical key, which names the card's parts.
    key: String,
    identifier: SharedString,
    /// The parent's title, for a sub-issue.
    parent: Option<SharedString>,
    title: SharedString,
    status: Option<Status>,
    priority: Option<i64>,
    project: Option<Project>,
    estimate: Option<u32>,
    progress: Option<Progress>,
    pull_requests: Vec<LinkedPullRequest>,
    /// Whether its page is open.
    open: bool,
}

impl Card {
    pub(super) fn of(item: &Item, open: bool) -> Self {
        let tracker = item.tracker.as_ref();
        Self {
            key: item.key.canonical(),
            identifier: item.identifier.clone().into(),
            parent: tracker
                .and_then(|tracker| tracker.parent.clone())
                .map(Into::into),
            title: item.title.clone().into(),
            status: status_of(item).cloned(),
            priority: item.priority,
            project: tracker.and_then(|tracker| tracker.project.clone()),
            estimate: tracker.and_then(|tracker| tracker.estimate),
            progress: tracker.and_then(|tracker| tracker.progress),
            pull_requests: tracker
                .map(|tracker| tracker.pull_requests.clone())
                .unwrap_or_default(),
            open,
        }
    }

    /// The card, named `id`; the caller handles its click. `link` makes
    /// the pull request chip, given the address it opens, open it.
    pub(super) fn render(
        self,
        id: impl Into<ElementId>,
        look: &Look,
        link: impl FnOnce(Stateful<Div>, String) -> Stateful<Div>,
    ) -> Stateful<Div> {
        let theme = &look.theme;
        let key = &self.key;
        let border = if self.open {
            theme.primary()
        } else {
            theme.active
        };
        let named = |part: &str, chip: Div| {
            let selector = format!("orchestrator-card-{part}-{key}");
            chip.debug_selector(move || selector.clone())
        };
        let mut chips: Vec<AnyElement> = vec![
            named(
                "priority",
                linear_parts::chip(look).child(linear_parts::priority_mark(look, self.priority)),
            )
            .into_any_element(),
        ];
        if let Some(project) = &self.project {
            chips.push(named("project", linear_parts::project(look, project)).into_any_element());
        }
        if let Some(estimate) = self.estimate {
            let chip = linear_parts::chip(look).child(linear_parts::estimate(look, estimate));
            chips.push(named("estimate", chip).into_any_element());
        }
        if let Some(progress) = self.progress {
            chips
                .push(named("progress", linear_parts::progress(look, progress)).into_any_element());
        }
        if let Some((chip, url)) = linear_parts::pull_request(
            look,
            &self.pull_requests,
            SharedString::from(format!("orchestrator-card-pr-{key}")),
            format!("orchestrator-card-pr-{key}"),
        ) {
            chips.push(link(chip, url).into_any_element());
        }
        div()
            .id(id)
            .flex_none()
            .px_3()
            .py_2p5()
            .flex()
            .flex_col()
            .gap_2()
            .rounded(px(corners::CONTROL))
            .border_1()
            .border_color(rgb(border))
            .bg(rgb(theme.surface))
            .shadow_sm()
            .cursor_pointer()
            .hover(|style| style.border_color(rgb(theme.muted)))
            .child(
                div()
                    .min_w_0()
                    .flex()
                    .items_center()
                    .gap_1()
                    .text_size(look.small())
                    .text_color(rgb(theme.muted))
                    .child(div().flex_none().child(self.identifier))
                    .when_some(self.parent, |line, parent| {
                        line.child(div().flex_none().child("\u{203a}"))
                            .child(div().min_w_0().truncate().child(parent))
                    }),
            )
            .child(
                div()
                    .flex()
                    .items_start()
                    .gap_2()
                    // The mark sits on the title's first line.
                    .child(
                        div()
                            .flex_none()
                            .pt(px(2.))
                            .child(linear_parts::state_icon(look, self.status.as_ref())),
                    )
                    .child(div().min_w_0().line_clamp(2).child(self.title)),
            )
            .child(div().flex().flex_wrap().gap_1().children(chips))
    }
}
