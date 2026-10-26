//! The small parts the Linear section's rows and cards share, drawn as
//! Linear draws them: a workflow state's mark, the priority mark, and the
//! outlined chips for linked pull requests, the project, the estimate, and
//! sub-issue progress. They need only the look, so a card dragged under the
//! pointer draws them too.

use super::{
    Look,
    look::{GREEN, MAGENTA, RED, YELLOW},
};
use crate::{
    config::mix,
    kanban,
    orchestrator::{LinkState, LinkedPullRequest, Progress, Project, Status, StatusKind},
};
use gpui::{prelude::*, *};

/// A state mark's size at the usual text size.
const STATE_ICON: f32 = 14.;

/// The color a status is drawn in: its tracker's, made readable on the
/// theme, else muted.
pub(super) fn status_hue(look: &Look, status: &Status) -> u32 {
    status
        .color
        .map_or(look.theme.muted, |color| look.theme.ink(color))
}

/// A workflow state's mark in its color, as Linear draws its kinds: a
/// dashed ring for the backlog, a ring for one not started, a ring part
/// filled for one started, and a filled circle for one done or canceled. A
/// muted ring stands for no state.
pub(super) fn state_icon(look: &Look, status: Option<&Status>) -> Div {
    let Some(status) = status else {
        return ring(look.theme.muted, STATE_ICON);
    };
    let hue = status_hue(look, status);
    let filled = |icon: &'static str| {
        look.dot(hue, STATE_ICON)
            .flex()
            .items_center()
            .justify_center()
            .child(look.icon(icon, STATE_ICON * 0.6, look.theme.text_on(hue)))
    };
    match status.kind {
        StatusKind::Backlog => ring(hue, STATE_ICON).border_dashed(),
        StatusKind::Started => ring(hue, STATE_ICON).child(look.dot(hue, STATE_ICON * 0.4)),
        StatusKind::Completed => filled("icons/check.svg"),
        StatusKind::Canceled => filled("icons/close.svg"),
        StatusKind::Triage | StatusKind::Unstarted | StatusKind::Other => ring(hue, STATE_ICON),
    }
}

/// A ring in `hue`, `size` across.
fn ring(hue: u32, size: f32) -> Div {
    div()
        .flex_none()
        .size(px(size))
        .rounded_full()
        .border_2()
        .border_color(rgb(hue))
        .flex()
        .items_center()
        .justify_center()
}

/// Linear's priority mark: three muted dashes for none, an orange square
/// with `!` for urgent, else one to three bars.
pub(super) fn priority_mark(look: &Look, priority: Option<i64>) -> Div {
    let cell = div()
        .flex_none()
        .w(px(16.))
        .flex()
        .items_center()
        .justify_center();
    match priority {
        Some(1) => {
            let fill = mix(look.hue(RED), look.hue(YELLOW), 40);
            cell.child(
                div()
                    .size(px(14.))
                    .rounded(px(3.))
                    .bg(rgb(fill))
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_size(px(10.))
                    .font_weight(FontWeight::BOLD)
                    .text_color(rgb(look.theme.text_on(fill)))
                    .child("!"),
            )
        }
        Some(priority @ 2..=4) => {
            let filled = usize::try_from(5 - priority).unwrap_or_default();
            cell.child(kanban::signal_bars(look, filled, 3, look.theme.subtext()))
        }
        _ => cell.child(div().flex().gap(px(1.5)).children((0..3).map(|_| {
            div()
                .w(px(3.))
                .h(px(1.5))
                .rounded(px(1.))
                .bg(rgb(look.theme.muted))
        }))),
    }
}

/// A rounded, outlined chip.
pub(super) fn chip(look: &Look) -> Div {
    div()
        .flex_none()
        .min_w_0()
        .h(px(look.row(24.)))
        .px(px(8.))
        .flex()
        .items_center()
        .gap(px(6.))
        .rounded_full()
        .border_1()
        .border_color(rgb(look.theme.active))
        .text_size(look.small())
        .text_color(rgb(look.theme.subtext()))
        .whitespace_nowrap()
}

/// What a pull request chip says, and the pull request it opens: the
/// liveliest, so one still open wins over merged ones.
pub(super) fn pull_request_chip(
    links: &[LinkedPullRequest],
) -> Option<(String, &LinkedPullRequest)> {
    let lead = links
        .iter()
        .min_by_key(|link| (link.state.is_none(), link.state))?;
    let label = match links {
        [one] => format!("#{}", one.number),
        _ => format!("{} PRs", links.len()),
    };
    Some((label, lead))
}

/// The chip of `links`, named `id` and `selector`, and the address it
/// opens; the caller makes the click open it.
pub(super) fn pull_request(
    look: &Look,
    links: &[LinkedPullRequest],
    id: impl Into<ElementId>,
    selector: String,
) -> Option<(Stateful<Div>, String)> {
    let (label, lead) = pull_request_chip(links)?;
    let hue = match lead.state {
        Some(LinkState::Open) => look.hue(GREEN),
        Some(LinkState::Merged) => look.hue(MAGENTA),
        Some(LinkState::Closed) => look.hue(RED),
        Some(LinkState::Draft) | None => look.theme.muted,
    };
    let border = look.theme.muted;
    let chip = chip(look)
        .id(id)
        .debug_selector(move || selector.clone())
        .role(Role::Button)
        .aria_label(format!("Open pull request {label}"))
        .cursor_pointer()
        .hover(move |style| style.border_color(rgb(border)))
        .child(look.icon("icons/pull-request.svg", 12., hue))
        .child(label)
        // The row or card under it neither opens nor starts a drag.
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation());
    Some((chip, lead.url.clone()))
}

/// The chip of `project`: its mark in its color, and its name.
pub(super) fn project(look: &Look, project: &Project) -> Div {
    let hue = project
        .color
        .map_or(look.theme.muted, |color| look.theme.ink(color));
    chip(look)
        .max_w(px(220.))
        .child(look.icon("icons/project.svg", 12., hue))
        .child(div().min_w_0().truncate().child(project.name.clone()))
}

/// The estimate mark and its number.
pub(super) fn estimate(look: &Look, estimate: u32) -> Div {
    let ink = look.theme.subtext();
    div()
        .flex_none()
        .flex()
        .items_center()
        .gap_1()
        .text_size(look.small())
        .text_color(rgb(ink))
        .child(look.icon("icons/estimate.svg", 12., ink))
        .child(estimate.to_string())
}

/// `done/total` of an item's sub-issues, `+` when more were left unread.
pub(super) fn progress_label(progress: Progress) -> String {
    let more = if progress.more { "+" } else { "" };
    format!("{}/{}{more}", progress.done, progress.total)
}

/// The chip of an item's sub-issue progress: a ring, full once every one
/// is done.
pub(super) fn progress(look: &Look, progress: Progress) -> Div {
    let all = progress.total > 0 && progress.done == progress.total && !progress.more;
    let hue = if all {
        look.hue(GREEN)
    } else {
        look.theme.muted
    };
    chip(look)
        .child(ring(hue, 12.).when(all, |ring| ring.bg(rgb(hue))))
        .child(progress_label(progress))
}
