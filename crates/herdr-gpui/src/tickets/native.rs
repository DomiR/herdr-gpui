//! The native board's state in a window, and reading it from herdr-dog.
//! While the section shows natively, a background read runs every
//! [`INTERVAL`], or at once after a pick, a move, or Refresh; drawing reads
//! what it left. herdr-dog announces changes only over its Effect RPC
//! socket, so the board polls instead. A failed read keeps what the last
//! one showed, with the error above it. Creating tickets and comments is
//! `super::edit`; opening a ticket's worktree is `super::worktree`.

use super::api::{Comment, Detail, Project, Server, Start, Status, Ticket};
use crate::{
    HerdrWindow,
    markdown_view::{self, Block},
    sidebar::ActivityPanel,
};
use gpui::{Context, FocusHandle, ListAlignment, ListState, SharedString, Task, Window, px};
use std::{
    collections::BTreeSet,
    rc::Rc,
    time::{Duration, Instant},
};

/// How often the board is read again while it shows.
const INTERVAL: Duration = Duration::from_secs(10);

/// The window's native board.
#[derive(Default)]
pub(crate) struct Native {
    /// The server the shown data came from; another address starts over.
    pub(super) server: Option<Server>,
    pub(super) projects: Vec<Project>,
    /// The project picked on the board, if any; the board falls back as
    /// [`super::board::project`] does.
    pub(super) picked: Option<i64>,
    /// The shown project's tickets.
    pub(super) tickets: Vec<Ticket>,
    /// The labels pressed to filter the lanes.
    pub(super) pressed: BTreeSet<String>,
    /// The ticket whose page shows instead of the lanes.
    pub(super) open: Option<i64>,
    pub(super) opened: Option<Rc<Opened>>,
    pub(super) error: Option<SharedString>,
    /// Why the last status change failed, kept past the reads after it
    /// until the next change.
    pub(super) notice: Option<SharedString>,
    pub(super) read_at: Option<Instant>,
    /// When the next read is due; `None` reads at the next tick.
    next: Option<Instant>,
    task: Option<Task<()>>,
    /// A status change in flight, which the next read follows.
    moving: Option<Task<()>>,
    /// The ticket being written, while the form shows.
    pub(super) draft: Option<super::edit::Draft>,
    /// The comment being written on the open ticket's page.
    pub(super) composer: Option<super::edit::Composer>,
    /// A create or a comment on its way to herdr-dog.
    pub(super) sending: Option<Task<()>>,
    /// What holds the keyboard while the section shows, so Cmd-Up and
    /// Cmd-Down reach the board and typing never reaches a terminal.
    pub(super) focus: Option<FocusHandle>,
    /// The lanes' and the page's virtualized lists.
    pub(super) lists: Lists,
    /// Whether the page's text runs the whole width rather than a readable
    /// column.
    pub(super) wide: bool,
}

/// How far past the visible rows a list lays out, so scrolling finds them.
const OVERDRAW: f32 = 400.;

/// The board's virtualized lists: only what shows is laid out each frame.
pub(crate) struct Lists {
    /// Each lane's cards, in lane order.
    pub(super) lanes: [ListState; 6],
    /// The open ticket's main column.
    pub(super) page: ListState,
    /// What the page's rows were last measured for; see `page_body`.
    pub(super) page_shape: std::cell::Cell<u64>,
}

impl Default for Lists {
    fn default() -> Self {
        let list = || ListState::new(0, ListAlignment::Top, px(OVERDRAW));
        Self {
            lanes: std::array::from_fn(|_| list()),
            page: list(),
            page_shape: std::cell::Cell::default(),
        }
    }
}

impl Lists {
    /// Measures every row again, as a new read, or a label pressed, may
    /// change any of them.
    pub(super) fn remeasure(&self) {
        for lane in &self.lanes {
            lane.remeasure();
        }
        self.page.remeasure();
    }
}

/// An open ticket's page, its Markdown parsed once as it is read.
pub(crate) struct Opened {
    pub(super) detail: Detail,
    pub(super) description: Rc<[Block]>,
    pub(super) comments: Vec<(Comment, Rc<[Block]>)>,
    /// The agent run herdr-dog started for it, if any.
    pub(super) start: Option<Start>,
}

/// One read's result, built off the UI thread.
struct Read {
    projects: Vec<Project>,
    tickets: Vec<Ticket>,
    opened: Option<ReadPage>,
}

/// An open ticket's page as read, its Markdown parsed off the UI thread.
struct ReadPage {
    detail: Detail,
    description: Vec<Block>,
    comments: Vec<(Comment, Vec<Block>)>,
    start: Option<Start>,
}

fn read(server: &Server, picked: Option<i64>, open: Option<i64>) -> super::Result<Read> {
    let projects = server.projects()?;
    let tickets = match super::board::project(&projects, picked) {
        Some(project) => server.tickets(project)?,
        None => Vec::new(),
    };
    let opened = match open {
        Some(number) => {
            let detail = server.detail(number)?;
            let comments = server.comments(number)?;
            let start = server.start(number)?;
            let description = markdown(server, &detail.ticket.description);
            let comments = comments
                .into_iter()
                .map(|comment| {
                    let lines = markdown(server, &comment.body);
                    (comment, lines)
                })
                .collect();
            Some(ReadPage {
                detail,
                description,
                comments,
                start,
            })
        }
        None => None,
    };
    Ok(Read {
        projects,
        tickets,
        opened,
    })
}

/// `body` as Markdown blocks, its attachments' relative addresses made
/// absolute against the server.
fn markdown(server: &Server, body: &str) -> Vec<Block> {
    let mut blocks = markdown_view::parse(body);
    if let Ok(base) = url::Url::parse(server.base().as_str()) {
        markdown_view::resolve_images(&mut blocks, &base);
    }
    blocks
}

impl Native {
    fn due(&self, now: Instant) -> bool {
        self.task.is_none() && self.next.is_none_or(|next| now >= next)
    }

    /// Reads at the next tick, dropping a read in flight, which may have
    /// asked for what was shown before.
    pub(super) fn read_now(&mut self) {
        self.task = None;
        self.next = None;
    }

    fn install(&mut self, read: super::Result<Read>, now: Instant) {
        self.task = None;
        self.next = Some(now + INTERVAL);
        let read = match read {
            Ok(read) => read,
            Err(error) => {
                self.error = Some(error.to_string().into());
                return;
            }
        };
        self.error = None;
        self.read_at = Some(now);
        self.projects = read.projects;
        self.tickets = read.tickets;
        self.lists.remeasure();
        self.opened = read
            .opened
            .filter(|page| Some(page.detail.ticket.number) == self.open)
            .map(|page| Opened {
                detail: page.detail,
                description: page.description.into(),
                comments: page
                    .comments
                    .into_iter()
                    .map(|(comment, lines)| (comment, lines.into()))
                    .collect(),
                start: page.start,
            })
            .map(Rc::new);
    }

    /// The project the board shows.
    pub(super) fn project(&self) -> Option<i64> {
        super::board::project(&self.projects, self.picked)
    }

    pub(super) fn reading(&self) -> bool {
        self.task.is_some() || self.moving.is_some() || self.sending.is_some()
    }
}

impl HerdrWindow {
    /// Runs every tick: reads the board while Tickets (native) shows and a
    /// read is due.
    pub(crate) fn poll_tickets(&mut self, cx: &mut Context<Self>) {
        if self.activity_section() != Some(ActivityPanel::TicketsNative) {
            return;
        }
        let Some(server) = self.config.tickets.api().map(Server::new) else {
            return;
        };
        if self.ticket_board.server.as_ref() != Some(&server) {
            self.ticket_board = Native {
                server: Some(server.clone()),
                focus: self.ticket_board.focus.take(),
                ..Default::default()
            };
            cx.notify();
        }
        let now = Instant::now();
        if !self.ticket_board.due(now) {
            return;
        }
        let picked = self.ticket_board.picked;
        let open = self.ticket_board.open;
        let work = cx
            .background_executor()
            .spawn(async move { read(&server, picked, open) });
        self.ticket_board.task = Some(cx.spawn(async move |this, cx| {
            let read = work.await;
            let _ = this.update(cx, |this, cx| {
                this.ticket_board.install(read, Instant::now());
                if let Some(opened) = this.ticket_board.opened.clone() {
                    let urls = markdown_view::image_urls(&opened.description)
                        .into_iter()
                        .chain(
                            opened
                                .comments
                                .iter()
                                .flat_map(|(_, blocks)| markdown_view::image_urls(blocks)),
                        );
                    markdown_view::request_images(urls, cx);
                }
                cx.notify();
            });
        }));
        cx.notify();
    }

    pub(super) fn refresh_tickets(&mut self, cx: &mut Context<Self>) {
        self.ticket_board.read_now();
        self.poll_tickets(cx);
    }

    pub(super) fn pick_ticket_project(&mut self, project: i64, cx: &mut Context<Self>) {
        let board = &mut self.ticket_board;
        if board.project() == Some(project) {
            return;
        }
        board.picked = Some(project);
        board.tickets.clear();
        board.pressed.clear();
        board.open = None;
        board.opened = None;
        board.draft = None;
        board.composer = None;
        board.lists.remeasure();
        self.refresh_tickets(cx);
    }

    pub(super) fn toggle_ticket_label(&mut self, label: &str, cx: &mut Context<Self>) {
        let pressed = &mut self.ticket_board.pressed;
        if !pressed.remove(label) {
            pressed.insert(label.to_owned());
        }
        self.ticket_board.lists.remeasure();
        cx.notify();
    }

    /// Shows ticket `number`'s page, or the lanes again with `None`.
    pub(super) fn open_ticket(&mut self, number: Option<i64>, cx: &mut Context<Self>) {
        let board = &mut self.ticket_board;
        if board.open == number {
            return;
        }
        board.open = number;
        board.opened = None;
        board.composer = None;
        board.lists.page.reset(0);
        match number {
            Some(_) => self.refresh_tickets(cx),
            None => cx.notify(),
        }
    }

    /// Gives the board the keyboard as its section shows.
    pub(crate) fn focus_native_tickets(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let focus = self
            .ticket_board
            .focus
            .get_or_insert_with(|| cx.focus_handle())
            .clone();
        window.focus(&focus, cx);
    }

    /// Cmd-Down scrolls to the end, Cmd-Up back to the start: of the open
    /// ticket's page, else of every lane. W switches the page's width.
    pub(super) fn native_tickets_key_down(
        &mut self,
        event: &gpui::KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let modifiers = event.keystroke.modifiers;
        // W on the page, while the board itself holds the keyboard rather
        // than a field, switches between Wide and Thin, as on herdr-dog.
        if event.keystroke.key == "w"
            && !modifiers.modified()
            && self.ticket_board.open.is_some()
            && self
                .ticket_board
                .focus
                .as_ref()
                .is_some_and(|focus| focus.is_focused(window))
        {
            self.toggle_ticket_width(cx);
            cx.stop_propagation();
            return;
        }
        if !modifiers.platform || modifiers.control || modifiers.alt || modifiers.shift {
            return;
        }
        let end = match event.keystroke.key.as_str() {
            "down" => true,
            "up" => false,
            _ => return,
        };
        let lists = &self.ticket_board.lists;
        let shown = match self.ticket_board.open {
            Some(_) => std::slice::from_ref(&lists.page),
            None => &lists.lanes[..],
        };
        for list in shown {
            if end {
                list.scroll_to_end();
            } else {
                list.scroll_to(gpui::ListOffset::default());
            }
        }
        cx.stop_propagation();
        cx.notify();
    }

    /// Moves ticket `number` to the end of `status`'s lane, then reads the
    /// board again.
    pub(super) fn set_ticket_status(
        &mut self,
        number: i64,
        status: Status,
        cx: &mut Context<Self>,
    ) {
        let Some(server) = self.ticket_board.server.clone() else {
            return;
        };
        let work = cx
            .background_executor()
            .spawn(async move { server.set_status(number, status) });
        self.ticket_board.moving = Some(cx.spawn(async move |this, cx| {
            let moved = work.await;
            let _ = this.update(cx, |this, cx| {
                let board = &mut this.ticket_board;
                board.moving = None;
                board.notice = moved.err().map(|error| error.to_string().into());
                this.refresh_tickets(cx);
            });
        }));
        cx.notify();
    }
}
