//! Writing to herdr-dog from the native board: a new ticket from the form
//! under the header, one line a field (`SearchInput`), where Enter creates
//! it and Escape drops it; and a comment or a reply on a ticket's page, in
//! a Markdown field like herdr-dog's composer (`TextArea`), where Cmd-Enter
//! sends and Escape drops it. One create or comment is in flight at a time,
//! and the board is read again once herdr-dog answers.

use super::api::{NewTicket, Status};
use crate::{
    HerdrWindow,
    search_input::SearchInput,
    text_area::{TextArea, TextAreaEvent},
};
use gpui::{prelude::*, *};

/// A ticket being written.
pub(crate) struct Draft {
    /// The lane it lands in.
    pub(super) status: Status,
    pub(super) title: Entity<SearchInput>,
    pub(super) description: Entity<SearchInput>,
}

/// A comment being written on the open ticket's page, in a Markdown field
/// like herdr-dog's: Cmd-Enter sends, Escape drops it.
pub(crate) struct Composer {
    /// The first comment of the thread it answers; `None` starts a thread.
    pub(super) thread: Option<i64>,
    pub(super) input: Entity<TextArea>,
    _events: Subscription,
}

impl HerdrWindow {
    fn ticket_input(&self, placeholder: &str, cx: &mut Context<Self>) -> Entity<SearchInput> {
        let font = self.config.ui.clone();
        let theme = self.theme.clone();
        let input = cx.new(SearchInput::new);
        input.update(cx, |input, cx| {
            input.set_placeholder(placeholder, cx);
            input.set_appearance(font, theme, cx);
        });
        input
    }

    /// Opens the form for a ticket in `status`'s lane, its title holding
    /// the keyboard. Another lane's form is replaced.
    pub(super) fn begin_ticket(
        &mut self,
        status: Status,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(draft) = &mut self.ticket_board.draft {
            draft.status = status;
            let focus = draft.title.read(cx).focus.clone();
            window.focus(&focus, cx);
            cx.notify();
            return;
        }
        let title = self.ticket_input("Ticket title", cx);
        let description = self.ticket_input("Description (optional)", cx);
        let focus = title.read(cx).focus.clone();
        self.ticket_board.draft = Some(Draft {
            status,
            title,
            description,
        });
        window.focus(&focus, cx);
        cx.notify();
    }

    /// Drops the form, handing the keyboard back to the board.
    pub(super) fn cancel_ticket(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.ticket_board.draft = None;
        self.focus_native_tickets(window, cx);
        cx.notify();
    }

    /// Creates the ticket the form holds in the shown project, then opens
    /// its page. A form without a title stays open.
    pub(super) fn create_ticket(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let board = &self.ticket_board;
        let (Some(draft), Some(server), Some(project)) =
            (&board.draft, board.server.clone(), board.project())
        else {
            return;
        };
        if board.sending.is_some() {
            return;
        }
        let title = draft.title.read(cx).text().trim().to_owned();
        if title.is_empty() {
            return;
        }
        let ticket = NewTicket {
            project_id: project,
            title,
            description: draft.description.read(cx).text().trim().to_owned(),
            status: draft.status,
        };
        let work = cx
            .background_executor()
            .spawn(async move { server.create(&ticket) });
        self.ticket_board.sending = Some(cx.spawn_in(window, async move |this, cx| {
            let created = work.await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.ticket_board.sending = None;
                match created {
                    Ok(ticket) => {
                        this.ticket_board.notice = None;
                        this.ticket_board.draft = None;
                        this.focus_native_tickets(window, cx);
                        this.open_ticket(Some(ticket.number), cx);
                    }
                    // The form keeps what was written, to send again.
                    Err(error) => this.ticket_board.notice = Some(error.to_string().into()),
                }
                this.refresh_tickets(cx);
            });
        }));
        cx.notify();
    }

    /// Opens the composer for a new thread, or for a reply to `thread`,
    /// keeping what was written when it moves to another thread.
    pub(super) fn begin_comment(
        &mut self,
        thread: Option<i64>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let placeholder = if thread.is_some() {
            "Reply to the thread"
        } else {
            "Write a comment"
        };
        let composer = match self.ticket_board.composer.take() {
            Some(composer) => Composer { thread, ..composer },
            None => {
                let font = self.config.ui.clone();
                let theme = self.theme.clone();
                let input = cx.new(TextArea::new);
                input.update(cx, |input, cx| input.set_appearance(font, theme, cx));
                let events =
                    cx.subscribe_in(&input, window, |this, _, event, window, cx| match event {
                        TextAreaEvent::Submit => this.send_comment(window, cx),
                        TextAreaEvent::Cancel => this.cancel_comment(window, cx),
                        TextAreaEvent::Changed => {}
                    });
                Composer {
                    thread,
                    input,
                    _events: events,
                }
            }
        };
        composer
            .input
            .update(cx, |input, cx| input.set_placeholder(placeholder, cx));
        let focus = composer.input.read(cx).focus.clone();
        self.ticket_board.composer = Some(composer);
        window.focus(&focus, cx);
        cx.notify();
    }

    pub(super) fn cancel_comment(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.ticket_board.composer = None;
        self.focus_native_tickets(window, cx);
        cx.notify();
    }

    /// Sends the composer's comment on the open ticket as the operator.
    pub(super) fn send_comment(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let board = &self.ticket_board;
        let (Some(composer), Some(server), Some(number)) =
            (&board.composer, board.server.clone(), board.open)
        else {
            return;
        };
        if board.sending.is_some() {
            return;
        }
        let body = composer.input.read(cx).text().trim().to_owned();
        if body.is_empty() {
            return;
        }
        let thread = composer.thread;
        let work = cx
            .background_executor()
            .spawn(async move { server.comment(number, &body, thread) });
        self.ticket_board.sending = Some(cx.spawn_in(window, async move |this, cx| {
            let sent = work.await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.ticket_board.sending = None;
                match sent {
                    Ok(_) => {
                        this.ticket_board.notice = None;
                        this.ticket_board.composer = None;
                        this.focus_native_tickets(window, cx);
                        // The new comment is the page's last.
                        this.ticket_board.lists.page.scroll_to_end();
                    }
                    Err(error) => this.ticket_board.notice = Some(error.to_string().into()),
                }
                this.refresh_tickets(cx);
            });
        }));
        cx.notify();
    }

    /// Keys the form's fields leave alone: Enter creates the ticket, Escape
    /// drops the form.
    pub(super) fn ticket_form_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let composing = self.ticket_board.draft.as_ref().is_some_and(|draft| {
            draft.title.read(cx).is_composing() || draft.description.read(cx).is_composing()
        });
        if composing || event.keystroke.modifiers.modified() {
            return;
        }
        match event.keystroke.key.as_str() {
            "enter" => self.create_ticket(window, cx),
            "escape" => self.cancel_ticket(window, cx),
            _ => return,
        }
        cx.stop_propagation();
        window.prevent_default();
    }
}
