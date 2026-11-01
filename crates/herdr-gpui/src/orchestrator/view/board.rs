//! The Linear section's board: the listed issues, as the search and
//! filters leave them, in a lane per workflow state, drawn with the ticket
//! board's parts (`crate::kanban`). A card opens the issue's page beside
//! the board, as a row does, and a card dragged to another lane or another
//! place in its own moves the issue there in Linear (`moves.rs`). A lane
//! lists its cards in Linear's own order, `sortOrder`. GitHub issues and
//! beads carry no status, so only Linear's issues have the board. Cards
//! are drawn as Linear's (`linear_card.rs`).

use super::{
    Event, IssueSource, Look, OrchestratorView, Tab, linear_card, linear_groups, linear_parts,
    rows, worktree,
};
use crate::{
    config::mix,
    fonts::StyledFont,
    kanban,
    orchestrator::{Item, SortOrder, Status, StatusKind},
};
use gpui::{prelude::*, *};
use std::{cmp::Ordering, collections::HashMap, rc::Rc, sync::Arc};

/// How the issues are laid out.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Layout {
    #[default]
    List,
    Board,
}

impl Layout {
    /// How it is stored with the window's chrome.
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::List => "list",
            Self::Board => "board",
        }
    }

    pub(crate) fn parse(name: &str) -> Option<Self> {
        match name {
            "list" => Some(Self::List),
            "board" => Some(Self::Board),
            _ => None,
        }
    }
}

/// `item`'s workflow state, when its tracker names one.
pub(super) fn status_of(item: &Item) -> Option<&Status> {
    item.tracker.as_ref()?.status.as_ref()
}

/// `item`'s place in its lane, when its tracker orders it by hand.
pub(super) fn sort_order_of(item: &Item) -> Option<SortOrder> {
    item.tracker.as_ref()?.sort_order
}

/// Each lane's status, `None` for items without one, and its items.
pub(super) type Lanes<'a> = Vec<(Option<&'a Status>, Vec<&'a Item>)>;

/// Lanes in board order, a lane for items without a status last, each
/// lane's items in their tracker's order, those without one last.
pub(super) fn lanes<'a>(items: impl IntoIterator<Item = &'a Item>) -> Lanes<'a> {
    let mut lanes: Lanes<'a> = Vec::new();
    for item in items {
        let status = status_of(item);
        let name = status.map(|status| status.name.as_str());
        match lanes
            .iter_mut()
            .find(|(lane, _)| lane.map(|lane| lane.name.as_str()) == name)
        {
            Some((_, items)) => items.push(item),
            None => lanes.push((status, vec![item])),
        }
    }
    sort(&mut lanes);
    for (_, items) in &mut lanes {
        // Stable, so items without an order keep the list's.
        items.sort_by_key(|item| {
            let order = sort_order_of(item);
            (order.is_none(), order)
        });
    }
    lanes
}

fn sort(lanes: &mut Lanes<'_>) {
    lanes.sort_by(|a, b| match (a.0, b.0) {
        (Some(a), Some(b)) => a.order().cmp(&b.order()),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    });
}

/// Adds an empty lane for each state of the listed issues' teams that no
/// issue is in, so a card can be dragged there. Canceled states stay off
/// the board, as Linear's own board leaves them.
pub(super) fn add_empty_lanes<'a>(
    lanes: &mut Lanes<'a>,
    items: impl IntoIterator<Item = &'a Item>,
) {
    // Every issue of a team shares its states, so each team is read once.
    let mut teams: Vec<*const Vec<Status>> = Vec::new();
    for tracker in items.into_iter().filter_map(|item| item.tracker.as_ref()) {
        let team = Arc::as_ptr(&tracker.states);
        if teams.contains(&team) {
            continue;
        }
        teams.push(team);
        for state in tracker.states.iter() {
            let shown = lanes
                .iter()
                .any(|(lane, _)| lane.is_some_and(|lane| lane.name == state.name));
            if state.kind != StatusKind::Canceled && !shown {
                lanes.push((Some(state), Vec::new()));
            }
        }
    }
    sort(lanes);
}

/// Where the board is scrolled: the row of lanes, and each lane's cards by
/// its status's name (`None` for the lane without one). Kept on the view,
/// as GPUI drops an element's own scroll state once a frame leaves it out,
/// so leaving the section for a worktree and coming back keeps them.
#[derive(Default)]
pub(super) struct Scrolls {
    row: ScrollHandle,
    lanes: HashMap<Option<String>, ScrollHandle>,
}

#[cfg(test)]
impl Scrolls {
    /// How far the row of lanes is scrolled.
    pub(super) fn row(&self) -> Point<Pixels> {
        self.row.offset()
    }

    /// How far the lane of the status named `name` is scrolled, once drawn.
    pub(super) fn lane(&self, name: Option<&str>) -> Option<Point<Pixels>> {
        self.lanes
            .get(&name.map(str::to_owned))
            .map(ScrollHandle::offset)
    }
}

/// The space under a lane's last card, which takes a card dropped at the
/// end.
const END_SLOT: f32 = 32.;

/// A card's place in its lane, or the space under the last: a clear top
/// edge, which a drop over it colors (`OrchestratorView::drop_slot`).
fn slot() -> Div {
    div().border_t_2().border_color(transparent_black())
}

/// A card on its way to another place, and how it looks under the pointer.
#[derive(Clone)]
pub(super) struct CardDrag {
    /// The issue's canonical key.
    pub(super) key: String,
    /// The lane it left, by status name.
    from: Option<String>,
    /// Its team's states, which name the lanes it can land in.
    states: Arc<Vec<Status>>,
    card: linear_card::Card,
    look: Rc<Look>,
}

impl CardDrag {
    /// Whether dropping it between the cards `above` and `below` (`None`
    /// at an end) of the lane of the state named `to` moves it: to another
    /// place in its own lane, or into another lane, of a state its team has.
    pub(super) fn lands(&self, to: &str, above: Option<&str>, below: Option<&str>) -> bool {
        if self.from.as_deref() == Some(to) {
            let me = Some(self.key.as_str());
            return above != me && below != me;
        }
        self.states
            .iter()
            .any(|state| state.name == to && state.id.is_some())
    }
}

impl Render for CardDrag {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let look = &self.look;
        div()
            .w(px(kanban::LANE_WIDTH - 16.))
            .opacity(0.9)
            .text_font(&look.ui)
            .text_size(look.size())
            .text_color(rgb(look.theme.foreground))
            .child(
                self.card
                    .clone()
                    .render("orchestrator-card-drag", look, |chip, _| chip),
            )
    }
}

impl OrchestratorView {
    /// Whether the issues show as a board.
    pub(super) fn shows_board(&self) -> bool {
        self.layout == Layout::Board
            && self.tab == Tab::Issues
            && self.issue_source == IssueSource::Linear
    }

    /// The color a status is drawn in: its tracker's, made readable on the
    /// theme, else muted.
    pub(super) fn status_hue(&self, status: &Status) -> u32 {
        linear_parts::status_hue(&self.look, status)
    }

    /// The status of `item` as a pill, for a row.
    pub(super) fn status_pill(&self, item: &Item) -> Option<Div> {
        let status = status_of(item)?;
        Some(kanban::status_pill(
            &self.look,
            status.name.clone(),
            self.status_hue(status),
        ))
    }

    /// Lays the Linear issues out as `layout`, as picked here or remembered
    /// by the window.
    pub(crate) fn set_layout(&mut self, layout: Layout, cx: &mut Context<Self>) {
        if self.layout == layout {
            return;
        }
        self.layout = layout;
        // The list folds groups the board shows whole.
        self.refresh_rows();
        cx.notify();
    }

    /// List and Board, in the Linear section's header.
    pub(super) fn render_layout_switch(&self, cx: &mut Context<Self>) -> Option<Div> {
        if self.tab != Tab::Issues || self.issue_source != IssueSource::Linear {
            return None;
        }
        let toggle = |id: &'static str, label: &'static str, layout: Layout| {
            self.look
                .toggle(id, label, self.layout == layout)
                .debug_selector(move || id.into())
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.set_layout(layout, cx);
                    // The window remembers it for the next run.
                    cx.emit(Event::Layout(layout));
                }))
        };
        Some(
            div()
                .flex()
                .gap_1()
                .child(toggle("orchestrator-layout-list", "List", Layout::List))
                .child(toggle("orchestrator-layout-board", "Board", Layout::Board)),
        )
    }

    /// The lanes the board shows, each with its rows in their tracker's
    /// order: the listed rows' states and, unless narrowed to one state,
    /// each other state of their teams, less the hidden ones.
    pub(super) fn board_lanes(&self) -> Vec<(Option<Status>, Vec<rows::ItemRow>)> {
        let shown: Vec<rows::ItemRow> = self
            .lines
            .iter()
            .filter_map(|line| match line {
                rows::Line::Row(row) => Some(*row),
                rows::Line::Heading { .. } => None,
            })
            .collect();
        let listed: HashMap<String, rows::ItemRow> = shown
            .iter()
            .map(|row| (self.row_item(row).key.canonical(), *row))
            .collect();
        // In list order, which a lane keeps for items without an order.
        let items = || shown.iter().map(|row| self.row_item(row));
        let mut lanes = lanes(items());
        // Narrowed to one state, the board shows only that state's lane.
        if self.status.is_none() {
            add_empty_lanes(&mut lanes, items());
        }
        lanes.retain(|(status, _)| {
            status.is_none_or(|status| !self.hidden_statuses.contains(&status.name))
        });
        lanes
            .into_iter()
            .map(|(status, items)| {
                let rows = items
                    .into_iter()
                    .filter_map(|item| listed.get(&item.key.canonical()).copied())
                    .collect();
                (status.cloned(), rows)
            })
            .collect()
    }

    pub(super) fn render_board(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let lanes = self.board_lanes();
        let scrolls: Vec<ScrollHandle> = lanes
            .iter()
            .map(|(status, _)| {
                let name = status.as_ref().map(|status| status.name.clone());
                self.board_scrolls.lanes.entry(name).or_default().clone()
            })
            .collect();
        let look = Rc::new(self.look.clone());
        let muted = self.look.theme.muted;
        let mut columns = Vec::with_capacity(lanes.len());
        for (index, ((status, rows), scroll)) in lanes.into_iter().zip(scrolls).enumerate() {
            let hue = status
                .as_ref()
                .map_or(muted, |status| self.status_hue(status));
            let keys: Vec<String> = rows
                .iter()
                .map(|row| self.row_item(row).key.canonical())
                .collect();
            let name = status.as_ref().map(|status| status.name.as_str());
            let mut cards: Vec<AnyElement> = Vec::with_capacity(rows.len() + 1);
            for (at, row) in rows.iter().enumerate() {
                let card = slot()
                    .flex_none()
                    .pt(px(6.))
                    .child(self.render_card(row, &look, cx));
                // Only a lane of a workflow state takes a card.
                cards.push(match name {
                    Some(to) => {
                        let above = at.checked_sub(1).map(|above| keys[above].as_str());
                        self.drop_slot(card, to, above, Some(&keys[at]), cx)
                            .into_any_element()
                    }
                    None => card.into_any_element(),
                });
            }
            if let Some(to) = name {
                let end = slot().flex_1().min_h(px(END_SLOT));
                let last = keys.last().map(String::as_str);
                cards.push(self.drop_slot(end, to, last, None, cx).into_any_element());
            }
            let estimate = linear_groups::estimate(rows.iter().map(|row| self.row_item(row)));
            let head = kanban::LaneHead {
                mark: linear_parts::state_icon(&self.look, status.as_ref()).into_any_element(),
                total: div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .children(estimate.map(|estimate| {
                        linear_parts::estimate(&self.look, estimate)
                            .debug_selector(move || format!("orchestrator-lane-estimate-{index}"))
                    }))
                    .child(self.look.muted(rows.len().to_string()))
                    .into_any_element(),
            };
            let lane = kanban::Lane {
                title: name.map_or_else(|| "No status".into(), |name| name.to_owned().into()),
                hue,
                count: rows.len(),
            }
            .render_with(
                "orchestrator",
                index,
                head,
                // A lane's "+" starts a new issue in its state.
                status
                    .as_ref()
                    .and_then(|status| status.id.clone())
                    .map(|state| {
                        self.new_issue_plus("orchestrator-lane", index, Some(state), cx)
                            .into_any_element()
                    }),
                // The slots space the cards, and mark where one lands.
                kanban::cards("orchestrator", index, cards)
                    .gap_0()
                    .track_scroll(&scroll),
                &self.look,
            );
            columns.push(match name {
                Some(to) => self.drop_target(lane, to, hue, cx),
                None => lane,
            });
        }
        kanban::lanes("orchestrator-board")
            .debug_selector(|| "orchestrator-board".into())
            .track_scroll(&self.board_scrolls.row)
            .children(columns)
            .into_any_element()
    }

    /// `lane`, of the state named `to`, taking a card dragged in from
    /// another lane, at its end; tinted while one is over it.
    fn drop_target(&self, lane: Div, to: &str, hue: u32, cx: &mut Context<Self>) -> Div {
        let wash = mix(self.look.theme.background, hue, 18);
        let (check, drop) = (to.to_owned(), to.to_owned());
        lane.drag_over::<CardDrag>(move |style, _, _, _| style.bg(rgb(wash)))
            .can_drop(move |value, _, _| {
                value.downcast_ref::<CardDrag>().is_some_and(|drag| {
                    drag.from.as_deref() != Some(check.as_str()) && drag.lands(&check, None, None)
                })
            })
            .on_drop(cx.listener(move |this, drag: &CardDrag, _, cx| {
                this.place_card(drag, &drop, None, cx);
            }))
    }

    /// `slot`, a card's place or the space under a lane's last card,
    /// taking a card dropped there: between `above` and `below` in the
    /// lane of the state named `to`. Its top edge shows as a line while a
    /// card it takes is over it, marking where the card would land.
    fn drop_slot(
        &self,
        slot: Div,
        to: &str,
        above: Option<&str>,
        below: Option<&str>,
        cx: &mut Context<Self>,
    ) -> Div {
        let line = self.look.theme.primary();
        let to = to.to_owned();
        let (above, below) = (above.map(str::to_owned), below.map(str::to_owned));
        let check = (to.clone(), above, below.clone());
        slot.drag_over::<CardDrag>(move |style, _, _, _| style.border_color(rgb(line)))
            .can_drop(move |value, _, _| {
                let (to, above, below) = &check;
                value
                    .downcast_ref::<CardDrag>()
                    .is_some_and(|drag| drag.lands(to, above.as_deref(), below.as_deref()))
            })
            .on_drop(cx.listener(move |this, drag: &CardDrag, _, cx| {
                this.place_card(drag, &to, below.as_deref(), cx);
            }))
    }

    fn render_card(
        &self,
        row: &rows::ItemRow,
        look: &Rc<Look>,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let item = self.row_item(row);
        let key = item.key.canonical();
        let tracker = item.tracker.as_ref();
        let selector = format!("orchestrator-card-{key}");
        // No assignee, cycle, or date, as the user asked to leave them off.
        let card = linear_card::Card::of(
            item,
            self.detail.as_ref().is_some_and(|detail| detail.key == key),
        );
        let drag = CardDrag {
            key: key.clone(),
            from: status_of(item).map(|status| status.name.clone()),
            states: tracker
                .map(|tracker| tracker.states.clone())
                .unwrap_or_default(),
            card: card.clone(),
            look: look.clone(),
        };
        let button = self.worktree_button(row, worktree::CARD_GROUP, cx);
        card.render(
            SharedString::from(selector.clone()),
            &self.look,
            |chip, url| {
                chip.on_click(cx.listener(move |_, _, _, cx| {
                    cx.emit(Event::OpenUrl(url.clone()));
                }))
            },
        )
        .debug_selector(move || selector.clone())
        .group(worktree::CARD_GROUP)
        .relative()
        .children(button.map(|button| button.absolute().top_1().right_1()))
        .on_drag(drag, |drag: &CardDrag, _, _, cx| cx.new(|_| drag.clone()))
        .on_click(cx.listener(move |this, _, _, cx| {
            this.selected = Some(key.clone());
            this.open_detail(key.clone(), cx);
        }))
    }
}
