//! One ticket's page on the native board: a row back to the lanes, with
//! Wide and Thin, then the main column (`super::page_body`) beside a column
//! of its properties, where a status is picked to move the ticket and the
//! worktree it was started in is opened.

use super::{
    api::{Status, Ticket},
    view::{button, priority_bars, status_hue},
};
use crate::{HerdrWindow, config::corners, orchestrator::Look};
use gpui::{prelude::*, *};

const SIDE_WIDTH: f32 = 288.;

impl HerdrWindow {
    pub(super) fn render_ticket_page(
        &self,
        number: i64,
        look: &Look,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let board = &self.ticket_board;
        // The card's ticket until the page is read, so the title shows at once.
        let opened = board.opened.as_ref();
        let ticket = opened
            .map(|opened| &opened.detail.ticket)
            .or_else(|| board.tickets.iter().find(|ticket| ticket.number == number));
        let reference = ticket
            .map(|ticket| super::board::reference(&board.projects, ticket))
            .unwrap_or_else(|| format!("#{number}"));
        let project = ticket
            .and_then(|ticket| {
                board
                    .projects
                    .iter()
                    .find(|project| project.id == ticket.project_id)
            })
            .map(|project| project.name.clone());
        let crumbs = div()
            .flex_none()
            .px_5()
            .py_2()
            .flex()
            .items_center()
            .gap_2()
            .border_b_1()
            .border_color(rgb(look.theme.active))
            .child(
                button(look, "ticket-back", "← Board")
                    .on_click(cx.listener(|this, _, _, cx| this.open_ticket(None, cx))),
            )
            .children(project.map(|name| look.muted(name)))
            .child(look.muted("›"))
            .child(look.mono(reference))
            .child(div().flex_1())
            .child(
                button(
                    look,
                    "ticket-width",
                    if board.wide { "Thin" } else { "Wide" },
                )
                .on_click(cx.listener(|this, _, _, cx| this.toggle_ticket_width(cx))),
            );
        let Some(ticket) = ticket else {
            return div()
                .flex()
                .flex_col()
                .flex_1()
                .min_h_0()
                .child(crumbs)
                .child(div().p_5().child(look.muted("Reading the ticket…")))
                .into_any_element();
        };
        let main = match opened {
            Some(opened) => self.render_page_body(opened, look, cx),
            // The card's title until the page is read.
            None => div()
                .flex_1()
                .min_w_0()
                .p_5()
                .flex()
                .flex_col()
                .gap_3()
                .child(
                    div()
                        .text_size(px(look.ui.size + 5.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(ticket.title.clone()),
                )
                .child(look.muted("Reading the ticket…"))
                .into_any_element(),
        };
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .child(crumbs)
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .child(main)
                    .child(self.ticket_properties(ticket, look, cx)),
            )
            .into_any_element()
    }

    fn ticket_properties(
        &self,
        ticket: &Ticket,
        look: &Look,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let number = ticket.number;
        let statuses = Status::LANES.iter().map(|status| {
            let status = *status;
            let current = ticket.status == status;
            div()
                .id(("ticket-status", status as usize))
                .debug_selector(move || format!("ticket-status-{}", status.label()))
                .px(px(8.))
                .h(px(22.))
                .flex()
                .items_center()
                .gap_1()
                .rounded(px(corners::SMALL))
                .border_1()
                .border_color(rgb(look.theme.active))
                .text_size(look.small())
                .when(current, |pill| pill.bg(rgb(look.theme.active)))
                .when(!current, |pill| {
                    pill.cursor_pointer()
                        .hover(|style| style.bg(rgb(look.theme.active)))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.set_ticket_status(number, status, cx)
                        }))
                })
                .child(look.dot(status_hue(look, status), 7.))
                .child(status.label())
        });
        let property = |name: &'static str, value: AnyElement| {
            div()
                .flex()
                .items_center()
                .gap_2()
                .child(div().w(px(72.)).flex_none().child(look.label(name)))
                .child(div().min_w_0().child(value))
        };
        let text = |value: &str| -> AnyElement {
            if value.is_empty() {
                look.muted("None").into_any_element()
            } else {
                div().truncate().child(value.to_owned()).into_any_element()
            }
        };
        let priority = div()
            .flex()
            .items_center()
            .gap_2()
            .child(priority_bars(look, ticket.priority))
            .child(ticket.priority.label())
            .into_any_element();
        let pull = ticket.pull.as_ref().map(|pull| {
            let url = pull.url.clone();
            div()
                .id("ticket-pull")
                .text_color(rgb(look.theme.primary()))
                .cursor_pointer()
                .child(format!("#{}", pull.number))
                .on_click(move |_, _, cx| cx.open_url(&url))
                .into_any_element()
        });
        div()
            .id("ticket-properties")
            .debug_selector(|| "ticket-properties".into())
            .flex_none()
            .w(px(SIDE_WIDTH))
            .h_full()
            .overflow_y_scroll()
            .p_4()
            .flex()
            .flex_col()
            .gap_3()
            .border_l_1()
            .border_color(rgb(look.theme.active))
            .child(look.label("STATUS"))
            .child(div().flex().flex_wrap().gap_1().children(statuses))
            .child(look.label("PROPERTIES"))
            .child(property("Priority", priority))
            .child(property("Assignee", text(&ticket.assignee)))
            .child(property("Kind", text(&ticket.kind)))
            .child(property("Size", text(&ticket.size)))
            .child(property("Branch", text(&ticket.branch)))
            .children(pull.map(|pull| property("Pull", pull)))
            .children(self.ticket_worktree(look, cx))
            .when(!ticket.labels.is_empty(), |side| {
                side.child(look.label("LABELS")).child(
                    div()
                        .flex()
                        .flex_wrap()
                        .gap_1()
                        .children(ticket.labels.iter().map(|label| look.chip(label.clone()))),
                )
            })
    }
}

impl HerdrWindow {
    /// Switches the page between text across the whole width and a
    /// readable column.
    pub(super) fn toggle_ticket_width(&mut self, cx: &mut Context<Self>) {
        self.ticket_board.wide = !self.ticket_board.wide;
        cx.notify();
    }

    /// The worktree row of the properties, once the ticket was started:
    /// where its agent run stands, and Open for a worktree on this machine.
    fn ticket_worktree(&self, look: &Look, cx: &mut Context<Self>) -> Option<Div> {
        let start = self.ticket_board.opened.as_ref()?.start.as_ref()?;
        let place = match (&start.worktree_key, start.local_worktree()) {
            (_, Some(path)) => path.to_owned(),
            (Some(key), None) => key.clone(),
            (None, None) => "Not created yet".to_owned(),
        };
        Some(
            div()
                .flex()
                .flex_col()
                .gap_1()
                .child(look.label("WORKTREE"))
                .child(look.mono(place).text_color(rgb(look.theme.subtext())))
                .child(look.muted(match &start.error {
                    Some(error) => format!("{}: {error}", start.stage),
                    None => start.stage.replace('-', " "),
                }))
                .when(start.local_worktree().is_some(), |row| {
                    row.child(
                        button(look, "ticket-worktree-open", "Open worktree").on_click(
                            cx.listener(|this, _, window, cx| {
                                this.open_ticket_worktree(window, cx)
                            }),
                        ),
                    )
                }),
        )
    }
}
