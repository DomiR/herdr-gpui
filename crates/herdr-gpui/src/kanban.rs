//! The board both issue boards draw: a row of lanes, one per status, each a
//! titled column of cards. Tickets (native) and the Linear section fill it
//! from their own items; the parts here know nothing of either, and leave
//! clicks to their caller.

use crate::{
    config::{corners, mix},
    orchestrator::Look,
};
use gpui::{prelude::*, *};

pub(crate) const LANE_WIDTH: f32 = 280.;

/// Four rising bars, `filled` of them inked, in red when `urgent`.
pub(crate) fn priority_bars(look: &Look, filled: usize, urgent: bool) -> Div {
    let ink = if urgent {
        look.hue(1)
    } else {
        look.theme.subtext()
    };
    signal_bars(look, filled, 4, ink)
}

/// `count` rising bars, `filled` of them in `ink`, as Linear's list draws
/// a priority with three.
pub(crate) fn signal_bars(look: &Look, filled: usize, count: usize, ink: u32) -> Div {
    let step = 7.5 / count.saturating_sub(1).max(1) as f32;
    div()
        .flex_none()
        .flex()
        .items_end()
        .gap(px(1.5))
        .h(px(12.))
        .children((0..count).map(|bar| {
            div()
                .w(px(2.5))
                .h(px(4. + step * bar as f32))
                .rounded(px(1.))
                .bg(rgb(if bar < filled { ink } else { look.theme.active }))
        }))
}

/// A status as a small colored word.
pub(crate) fn status_pill(look: &Look, name: impl Into<SharedString>, hue: u32) -> Div {
    div()
        .flex_none()
        .flex()
        .items_center()
        .gap_1()
        .px(px(6.))
        .rounded(px(corners::SMALL))
        .bg(rgb(mix(look.theme.background, hue, 16)))
        .text_size(look.tiny())
        .text_color(rgb(look.theme.subtext()))
        .child(look.dot(hue, 6.))
        .child(name.into())
}

/// The row the lanes sit in, scrolling sideways.
pub(crate) fn lanes(id: impl Into<ElementId>) -> Stateful<Div> {
    div()
        .id(id)
        .flex_1()
        .min_h_0()
        .p_4()
        .flex()
        .gap_3()
        .overflow_x_scroll()
}

/// What a lane's head says.
pub(crate) struct Lane {
    pub(crate) title: SharedString,
    pub(crate) hue: u32,
    pub(crate) count: usize,
}

impl Lane {
    /// The lane, named `name` for its selectors, with `action` at the end
    /// of its head and `body`, its cards, under it: [`cards`], or a
    /// virtualized list for a long lane.
    pub(crate) fn render(
        self,
        name: &'static str,
        index: usize,
        action: Option<AnyElement>,
        body: impl IntoElement,
        look: &Look,
    ) -> Div {
        let head = LaneHead {
            mark: look.dot(self.hue, 8.).into_any_element(),
            total: look.muted(self.count.to_string()).into_any_element(),
        };
        self.render_with(name, index, head, action, body, look)
    }

    /// [`Lane::render`] with `head`'s mark and total in place of the dot
    /// in the lane's hue and the count.
    pub(crate) fn render_with(
        self,
        name: &'static str,
        index: usize,
        head: LaneHead,
        action: Option<AnyElement>,
        body: impl IntoElement,
        look: &Look,
    ) -> Div {
        div()
            .debug_selector(move || format!("{name}-lane-{index}"))
            .flex_none()
            .w(px(LANE_WIDTH))
            .h_full()
            .flex()
            .flex_col()
            .rounded(px(corners::CONTROL))
            .bg(rgb(mix(look.theme.background, self.hue, 6)))
            .child(
                div()
                    .debug_selector(move || format!("{name}-lane-head-{index}"))
                    .flex_none()
                    .px_3()
                    .py_2()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(head.mark)
                    .child(
                        div()
                            .min_w_0()
                            .truncate()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(self.title),
                    )
                    .child(head.total)
                    .child(div().flex_1())
                    .children(action),
            )
            .child(div().flex_1().min_h_0().flex().flex_col().child(body))
    }
}

/// What leads a lane's head and follows its title.
pub(crate) struct LaneHead {
    pub(crate) mark: AnyElement,
    pub(crate) total: AnyElement,
}

/// A lane's cards in one scrolling column, every one laid out: for lanes
/// short enough not to need a virtualized list.
pub(crate) fn cards(
    name: &'static str,
    index: usize,
    cards: impl IntoIterator<Item = impl IntoElement>,
) -> Stateful<Div> {
    div()
        .id((SharedString::from(format!("{name}-lane-cards")), index))
        .flex_1()
        .min_h_0()
        .px_2()
        .pb_2()
        .flex()
        .flex_col()
        .gap_2()
        .overflow_y_scroll()
        .children(cards)
}

/// What a card shows.
#[derive(Clone, Default)]
pub(crate) struct Card {
    pub(crate) reference: SharedString,
    pub(crate) title: SharedString,
    /// Priority bars inked, out of four.
    pub(crate) priority: usize,
    pub(crate) urgent: bool,
    pub(crate) labels: Vec<SharedString>,
    /// The status, as name and color, for a board whose lanes are not
    /// its statuses.
    pub(crate) status: Option<(SharedString, u32)>,
    /// Short words along the bottom, such as kind and assignee.
    pub(crate) footer: Vec<SharedString>,
    /// Done and total sub-items.
    pub(crate) progress: Option<(usize, usize)>,
    /// Whether its page is open.
    pub(crate) open: bool,
}

impl Card {
    /// The card; the caller names it and handles its click.
    pub(crate) fn render(self, id: impl Into<ElementId>, look: &Look) -> Stateful<Div> {
        let more = self.labels.len().saturating_sub(2);
        let border = if self.open {
            look.theme.primary()
        } else {
            look.theme.active
        };
        div()
            .id(id)
            .flex_none()
            .p_2()
            .flex()
            .flex_col()
            .gap_1()
            .rounded(px(corners::CONTROL))
            .border_1()
            .border_color(rgb(border))
            .bg(rgb(look.theme.background))
            .cursor_pointer()
            .hover(|style| style.border_color(rgb(look.theme.muted)))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(priority_bars(look, self.priority, self.urgent))
                    .child(look.mono(self.reference).text_color(rgb(look.theme.muted)))
                    .child(div().flex_1())
                    .children(self.status.map(|(name, hue)| status_pill(look, name, hue))),
            )
            .child(div().line_clamp(2).child(self.title))
            .when(!self.labels.is_empty(), |card| {
                card.child(
                    div()
                        .flex()
                        .gap_1()
                        .children(
                            self.labels
                                .into_iter()
                                .take(2)
                                .map(|label| look.chip(label)),
                        )
                        .when(more > 0, |row| row.child(look.muted(format!("+{more}")))),
                )
            })
            .when(!self.footer.is_empty() || self.progress.is_some(), |card| {
                card.child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .text_size(look.tiny())
                        .text_color(rgb(look.theme.muted))
                        .children(self.footer)
                        .child(div().flex_1())
                        .when_some(self.progress, |row, (done, total)| {
                            row.child(format!("{done}/{total}"))
                        }),
                )
            })
    }
}
