//! Drawing the native board: a header with the projects, then either the
//! six status lanes of the shown project's cards or one ticket's page
//! (`super::page`). Everything is drawn from the last read.

use super::{
    api::{Priority, Status},
    board::{self, Card},
};
use crate::{HerdrWindow, config::corners, fonts::StyledFont, kanban, orchestrator::Look};
use gpui::{prelude::*, *};
use std::rc::Rc;

// The theme's ANSI colors, by palette index, as the orchestrator uses them.
pub(super) const RED: usize = 1;
const GREEN: usize = 2;
const YELLOW: usize = 3;
const BLUE: usize = 4;
const MAGENTA: usize = 5;
const CYAN: usize = 6;

/// The color a status is drawn in.
pub(super) fn status_hue(look: &Look, status: Status) -> u32 {
    match status {
        Status::Backlog | Status::Other => look.theme.muted,
        Status::Ready => look.hue(CYAN),
        Status::Todo => look.hue(BLUE),
        Status::InProgress => look.hue(YELLOW),
        Status::Review => look.hue(MAGENTA),
        Status::Done => look.hue(GREEN),
    }
}

/// The priority's bars, as both boards draw them.
pub(super) fn priority_bars(look: &Look, priority: Priority) -> Div {
    kanban::priority_bars(look, priority.bars(), priority == Priority::Critical)
}

/// A small button with a word on it.
pub(super) fn button(
    look: &Look,
    id: &'static str,
    text: impl Into<SharedString>,
) -> Stateful<Div> {
    div()
        .id(id)
        .debug_selector(move || id.into())
        .flex_none()
        .px(px(10.))
        .h(px(24.))
        .flex()
        .items_center()
        .gap_1()
        .rounded(px(corners::CONTROL))
        .border_1()
        .border_color(rgb(look.theme.active))
        .text_size(look.small())
        .cursor_pointer()
        .hover(|style| style.bg(rgb(look.theme.active)))
        .child(text.into())
}

impl HerdrWindow {
    fn ticket_look(&self) -> Look {
        Look {
            theme: self.theme.clone(),
            ui: self.config.ui.clone(),
            mono: self.config.terminal.clone(),
        }
    }

    pub(crate) fn render_native_tickets(&self, cx: &mut Context<Self>) -> AnyElement {
        let look = self.ticket_look();
        let board = &self.ticket_board;
        let body = match board.open {
            Some(number) => self.render_ticket_page(number, &look, cx),
            None => self.render_ticket_lanes(&look, cx),
        };
        let notices = [board.notice.clone(), board.error.clone()]
            .into_iter()
            .flatten()
            .map(|text| {
                div()
                    .debug_selector(|| "tickets-error".into())
                    .flex_none()
                    .px_5()
                    .py_2()
                    .bg(rgb(look.wash(RED)))
                    .text_color(rgb(look.hue(RED)))
                    .text_size(look.small())
                    .child(text)
            });
        div()
            .id("tickets-native")
            .debug_selector(|| "tickets-native".into())
            .when_some(board.focus.clone(), |section, focus| {
                section.track_focus(&focus)
            })
            .on_key_down(cx.listener(Self::native_tickets_key_down))
            .flex()
            .flex_col()
            .flex_1()
            .size_full()
            .min_w_0()
            .min_h_0()
            .bg(rgb(look.theme.background))
            .text_color(rgb(look.theme.foreground))
            .text_size(look.size())
            .child(self.render_tickets_header(&look, cx))
            .children(self.render_ticket_form(&look, cx))
            .children(notices)
            .child(body)
            .into_any_element()
    }

    fn render_tickets_header(&self, look: &Look, cx: &mut Context<Self>) -> Div {
        let board = &self.ticket_board;
        let shown = board.project();
        let projects = board.projects.iter().map(|project| {
            let id = project.id;
            let selected = shown == Some(id);
            div()
                .id(("tickets-project", id.unsigned_abs()))
                .debug_selector(move || format!("tickets-project-{id}"))
                .flex_none()
                .px(px(8.))
                .h(px(24.))
                .flex()
                .items_center()
                .gap_1()
                .rounded(px(corners::CONTROL))
                .cursor_pointer()
                .when(selected, |pill| pill.bg(rgb(look.theme.active)))
                .hover(|style| style.bg(rgb(look.theme.active)))
                .child(project.name.clone())
                .child(
                    div()
                        .text_font(&look.mono)
                        .text_size(look.tiny())
                        .text_color(rgb(look.theme.muted))
                        .child(project.prefix.clone()),
                )
                .on_click(cx.listener(move |this, _, _, cx| this.pick_ticket_project(id, cx)))
        });
        let count = (!board.tickets.is_empty())
            .then(|| look.muted(format!("{} tickets", board.tickets.len())));
        let url = self.config.tickets.url();
        div()
            .debug_selector(|| "tickets-header".into())
            .flex_none()
            .px_5()
            .py_3()
            .flex()
            .items_center()
            .gap_2()
            .border_b_1()
            .border_color(rgb(look.theme.active))
            .child(
                div()
                    .flex_none()
                    .mr_2()
                    .text_size(px(look.ui.size + 3.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("Tickets"),
            )
            .child(
                div()
                    .id("tickets-projects")
                    .flex()
                    .min_w_0()
                    .gap_1()
                    .overflow_x_scroll()
                    .children(projects),
            )
            .child(div().flex_1())
            .children(count)
            .when(shown.is_some(), |header| {
                header.child(
                    button(look, "tickets-new", "New ticket").on_click(cx.listener(
                        |this, _, window, cx| this.begin_ticket(Status::Backlog, window, cx),
                    )),
                )
            })
            .child(
                button(
                    look,
                    "tickets-refresh",
                    if board.reading() {
                        "Reading…"
                    } else {
                        "Refresh"
                    },
                )
                .on_click(cx.listener(|this, _, _, cx| this.refresh_tickets(cx))),
            )
            .when_some(url, |header, url| {
                header.child(
                    button(look, "tickets-open-web", "Open in browser")
                        .on_click(cx.listener(move |_, _, _, cx| cx.open_url(url.as_str()))),
                )
            })
    }

    fn render_ticket_lanes(&self, look: &Look, cx: &mut Context<Self>) -> AnyElement {
        let board = &self.ticket_board;
        if board.projects.is_empty() {
            let text = if board.read_at.is_none() && board.error.is_none() {
                "Reading herdr-dog's tickets…"
            } else {
                "herdr-dog has no projects yet. Add one in its web UI."
            };
            return div()
                .debug_selector(|| "tickets-empty".into())
                .flex_1()
                .flex()
                .items_center()
                .justify_center()
                .text_color(rgb(look.theme.muted))
                .child(text)
                .into_any_element();
        }
        let labels = board::labels(&board.tickets).into_iter().map(|label| {
            let pressed = board.pressed.contains(label);
            let name = SharedString::from(label.to_owned());
            div()
                .id(name.clone())
                .flex_none()
                .px(px(8.))
                .h(px(20.))
                .flex()
                .items_center()
                .rounded(px(corners::SMALL))
                .border_1()
                .border_color(rgb(look.theme.active))
                .text_size(look.small())
                .cursor_pointer()
                .when(pressed, |chip| {
                    chip.bg(rgb(look.theme.active))
                        .text_color(rgb(look.theme.foreground))
                })
                .when(!pressed, |chip| chip.text_color(rgb(look.theme.subtext())))
                .child(name.clone())
                .on_click(cx.listener(move |this, _, _, cx| this.toggle_ticket_label(&name, cx)))
        });
        let lanes = board::lanes(&board.tickets, &board.pressed);
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .child(
                div()
                    .id("tickets-labels")
                    .flex_none()
                    .px_5()
                    .pt_3()
                    .flex()
                    .gap_1()
                    .overflow_x_scroll()
                    .children(labels),
            )
            .child(
                kanban::lanes("tickets-lanes")
                    .debug_selector(|| "tickets-lanes".into())
                    .children(
                        lanes
                            .into_iter()
                            .enumerate()
                            .map(|(index, (status, cards))| {
                                self.render_ticket_lane(index, status, &cards, look, cx)
                            }),
                    ),
            )
            .into_any_element()
    }

    fn render_ticket_lane(
        &self,
        index: usize,
        status: Status,
        cards: &[Card<'_>],
        look: &Look,
        cx: &mut Context<Self>,
    ) -> Div {
        let add = div()
            .id(("tickets-lane-new", index))
            .debug_selector(move || format!("tickets-lane-new-{index}"))
            .px_1()
            .rounded(px(corners::SMALL))
            .text_color(rgb(look.theme.muted))
            .cursor_pointer()
            .hover(|style| style.bg(rgb(look.theme.active)))
            .child("+")
            .on_click(cx.listener(move |this, _, window, cx| this.begin_ticket(status, window, cx)))
            .into_any_element();
        // Each card is drawn only while it shows: a lane can hold a
        // project's whole history.
        let shown: Rc<Vec<(i64, kanban::Card)>> = Rc::new(
            cards
                .iter()
                .map(|card| (card.ticket.number, self.ticket_card(card)))
                .collect(),
        );
        let state = self.ticket_board.lists.lanes[index].clone();
        if state.item_count() != shown.len() {
            state.reset(shown.len());
        }
        let look_of = look.clone();
        let this = cx.weak_entity();
        let body = list(state, move |at, _, _| {
            let Some((number, card)) = shown.get(at).cloned() else {
                return div().into_any_element();
            };
            let this = this.clone();
            div()
                .px_2()
                .pb_2()
                .child(
                    card.render(("ticket-card", number.unsigned_abs()), &look_of)
                        .debug_selector(move || format!("ticket-card-{number}"))
                        .on_click(move |_, _, cx| {
                            let _ = this.update(cx, |this, cx| this.open_ticket(Some(number), cx));
                        }),
                )
                .into_any_element()
        })
        .size_full();
        kanban::Lane {
            title: status.label().into(),
            hue: status_hue(look, status),
            count: cards.len(),
        }
        .render("tickets", index, Some(add), body, look)
    }

    /// What `card` shows.
    fn ticket_card(&self, card: &Card<'_>) -> kanban::Card {
        let ticket = card.ticket;
        let mut footer: Vec<SharedString> = vec![ticket.kind.clone().into()];
        if !ticket.assignee.is_empty() {
            footer.push(format!("@{}", ticket.assignee).into());
        }
        kanban::Card {
            reference: board::reference(&self.ticket_board.projects, ticket).into(),
            title: ticket.title.clone().into(),
            priority: ticket.priority.bars(),
            urgent: ticket.priority == Priority::Critical,
            labels: ticket.labels.iter().cloned().map(Into::into).collect(),
            status: None,
            footer,
            progress: card.progress,
            open: false,
        }
    }
}

impl HerdrWindow {
    /// The form for a new ticket under the header, while one is written.
    fn render_ticket_form(&self, look: &Look, cx: &mut Context<Self>) -> Option<Div> {
        let draft = self.ticket_board.draft.as_ref()?;
        let sending = self.ticket_board.sending.is_some();
        Some(
            div()
                .debug_selector(|| "tickets-form".into())
                .flex_none()
                .px_5()
                .py_3()
                .flex()
                .items_center()
                .gap_2()
                .border_b_1()
                .border_color(rgb(look.theme.active))
                .on_key_down(cx.listener(Self::ticket_form_key_down))
                .child(look.dot(status_hue(look, draft.status), 8.))
                .child(look.muted(format!("New in {}", draft.status.label())))
                .child(div().flex_1().min_w_0().child(draft.title.clone()))
                .child(div().flex_1().min_w_0().child(draft.description.clone()))
                .child(
                    button(
                        look,
                        "tickets-form-create",
                        if sending { "Creating…" } else { "Create" },
                    )
                    .on_click(cx.listener(|this, _, window, cx| this.create_ticket(window, cx))),
                )
                .child(
                    button(look, "tickets-form-cancel", "Cancel").on_click(
                        cx.listener(|this, _, window, cx| this.cancel_ticket(window, cx)),
                    ),
                ),
        )
    }
}
