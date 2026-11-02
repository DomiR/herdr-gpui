//! The open ticket's main column, as a virtualized list: the title, each
//! line of the description, the sub-tickets, the relations, and each
//! comment with its thread's reply. Only what shows is laid out, so a long
//! ticket costs a frame no more than a short one; a ticket page laid out
//! whole cost tens of milliseconds a frame. The list draws from the page's
//! data and calls back into the window through a weak handle, as its rows
//! are built outside the window's render.

use super::{
    api::{Comment, Status},
    native::Opened,
    view::{button, status_hue},
};
use crate::{
    HerdrWindow,
    config::corners,
    markdown_view::{self, Block as Markdown},
    orchestrator::Look,
    text_area::TextArea,
};
use gpui::{prelude::*, *};
use std::rc::Rc;

/// How wide the main column's text runs in the thin layout.
pub(super) const THIN_WIDTH: f32 = 760.;

/// A comment with its Markdown blocks.
pub(super) type Entry = (Comment, Rc<[Markdown]>);

/// One row of the main column.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Block {
    Title,
    /// Block `n` of the description.
    Description(usize),
    NoDescription,
    SubTickets,
    Relations,
    CommentsHeading,
    /// Comment `n`, in thread order.
    Comment(usize),
    /// The end of a thread: its reply, or the composer answering it.
    Reply(i64),
    /// The end of the page: a new thread, or its composer.
    NewThread,
}

/// The comments in threads, each thread in the order it began, its replies
/// oldest first under its first comment.
pub(super) fn threads(opened: &Opened) -> Vec<(i64, Vec<&Entry>)> {
    let mut threads: Vec<(i64, Vec<&Entry>)> = Vec::new();
    for entry in &opened.comments {
        let thread = entry.0.thread;
        match threads.iter_mut().find(|(id, _)| *id == thread) {
            Some((_, comments)) => comments.push(entry),
            None => threads.push((thread, vec![entry])),
        }
    }
    threads
}

/// The rows of `opened`'s page, in order, and the comments in thread order.
fn blocks(opened: &Opened) -> (Vec<Block>, Vec<usize>) {
    let mut blocks = vec![Block::Title];
    if opened.description.is_empty() {
        blocks.push(Block::NoDescription);
    } else {
        blocks.extend((0..opened.description.len()).map(Block::Description));
    }
    if !opened.detail.sub_tickets.is_empty() {
        blocks.push(Block::SubTickets);
    }
    if !opened.detail.relations.is_empty() {
        blocks.push(Block::Relations);
    }
    blocks.push(Block::CommentsHeading);
    let mut order = Vec::with_capacity(opened.comments.len());
    for (thread, comments) in threads(opened) {
        for entry in comments {
            let at = opened
                .comments
                .iter()
                .position(|candidate| std::ptr::eq(candidate, entry))
                .unwrap_or_default();
            blocks.push(Block::Comment(order.len()));
            order.push(at);
        }
        blocks.push(Block::Reply(thread));
    }
    blocks.push(Block::NewThread);
    (blocks, order)
}

/// What the list's rows draw from, shared by every row of a frame.
struct Body {
    opened: Rc<Opened>,
    blocks: Vec<Block>,
    /// Each `Block::Comment`'s index into `opened.comments`.
    order: Vec<usize>,
    /// Each sub-ticket's `PREFIX-N`.
    sub_refs: Vec<String>,
    look: Look,
    composer: Option<(Option<i64>, Entity<TextArea>)>,
    sending: bool,
    wide: bool,
    this: WeakEntity<HerdrWindow>,
}

impl HerdrWindow {
    /// The main column of the open, read ticket.
    pub(super) fn render_page_body(
        &self,
        opened: &Rc<Opened>,
        look: &Look,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let board = &self.ticket_board;
        let (blocks, order) = blocks(opened);
        let state = board.lists.page.clone();
        // Rows keep the height they were measured at, so anything that
        // changes how they draw measures them again.
        let shape = {
            use std::hash::{Hash, Hasher};
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            (
                Rc::as_ptr(opened) as usize,
                board
                    .composer
                    .as_ref()
                    .map(|composer| (composer.thread, composer.input.read(cx).shape())),
                board.sending.is_some(),
                board.wide,
                look.ui.size.to_bits(),
            )
                .hash(&mut hasher);
            hasher.finish()
        };
        let reshaped = board.lists.page_shape.replace(shape) != shape;
        if state.item_count() != blocks.len() {
            state.reset(blocks.len());
        } else if reshaped {
            state.remeasure();
        }
        let body = Rc::new(Body {
            opened: opened.clone(),
            blocks,
            order,
            sub_refs: opened
                .detail
                .sub_tickets
                .iter()
                .map(|sub| super::board::reference(&board.projects, sub))
                .collect(),
            look: look.clone(),
            composer: board
                .composer
                .as_ref()
                .map(|composer| (composer.thread, composer.input.clone())),
            sending: board.sending.is_some(),
            wide: board.wide,
            this: cx.weak_entity(),
        });
        div()
            .debug_selector(|| "ticket-main".into())
            .flex_1()
            .min_w_0()
            .h_full()
            .flex()
            .flex_col()
            .child(list(state, move |index, window, cx| body.row(index, window, cx)).size_full())
            .into_any_element()
    }
}

impl Body {
    fn row(&self, index: usize, _: &mut Window, cx: &mut App) -> AnyElement {
        let Some(block) = self.blocks.get(index).copied() else {
            return div().into_any_element();
        };
        let look = &self.look;
        let content = match block {
            Block::Title => div()
                .pt_5()
                .pb_3()
                .text_size(px(look.ui.size + 5.))
                .font_weight(FontWeight::SEMIBOLD)
                .child(self.opened.detail.ticket.title.clone()),
            Block::Description(at) => match self.opened.description.get(at) {
                Some(block) => div()
                    .debug_selector(move || format!("ticket-description-{at}"))
                    .pb_2()
                    .child(markdown_view::block(
                        "ticket-description",
                        at,
                        block,
                        look,
                        cx,
                    )),
                None => div(),
            },
            Block::NoDescription => look.muted("No description."),
            Block::SubTickets => self.sub_tickets(),
            Block::Relations => self.relations(),
            Block::CommentsHeading => {
                let title = match self.opened.comments.len() {
                    0 => "No comments".to_owned(),
                    1 => "1 comment".to_owned(),
                    count => format!("{count} comments"),
                };
                div()
                    .pt_5()
                    .pb_2()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(title)
            }
            Block::Comment(at) => match self
                .order
                .get(at)
                .and_then(|at| self.opened.comments.get(*at))
            {
                Some((comment, blocks)) => {
                    div().pb_2().child(comment_card(comment, blocks, look, cx))
                }
                None => div(),
            },
            Block::Reply(thread) => div().ml_6().pb_3().child(self.reply(thread)),
            Block::NewThread => div().pt_2().pb_5().child(self.new_thread()),
        };
        // The thin layout keeps the text to a readable width.
        div()
            .w_full()
            .px_5()
            .flex()
            .justify_center()
            .child(
                content
                    .w_full()
                    .min_w_0()
                    .when(!self.wide, |content| content.max_w(px(THIN_WIDTH))),
            )
            .into_any_element()
    }

    fn sub_tickets(&self) -> Div {
        let look = &self.look;
        let subs = &self.opened.detail.sub_tickets;
        let done = subs.iter().filter(|sub| sub.status == Status::Done).count();
        let rows = subs.iter().enumerate().map(|(index, sub)| {
            let number = sub.number;
            let this = self.this.clone();
            div()
                .id(("ticket-sub", number.unsigned_abs()))
                .px_2()
                .py_1()
                .flex()
                .items_center()
                .gap_2()
                .rounded(px(corners::SMALL))
                .cursor_pointer()
                .hover(|style| style.bg(rgb(look.theme.active)))
                .child(look.dot(status_hue(look, sub.status), 8.))
                .child(
                    look.mono(self.sub_refs.get(index).cloned().unwrap_or_default())
                        .text_color(rgb(look.theme.muted)),
                )
                .child(div().min_w_0().truncate().child(sub.title.clone()))
                .on_click(move |_, _, cx| {
                    let _ = this.update(cx, |this, cx| this.open_ticket(Some(number), cx));
                })
        });
        section(look, format!("Sub-tickets {done}/{}", subs.len())).children(rows)
    }

    fn relations(&self) -> Div {
        let look = &self.look;
        section(look, "Relations").children(self.opened.detail.relations.iter().map(|relation| {
            div()
                .flex()
                .items_center()
                .gap_2()
                .child(look.muted(relation.kind.replace('_', " ")))
                .child(look.mono(relation.reference.clone()))
                .child(div().min_w_0().truncate().child(relation.title.clone()))
        }))
    }

    fn reply(&self, thread: i64) -> Div {
        if self.composer.as_ref().map(|(answers, _)| *answers) == Some(Some(thread)) {
            return self.composer();
        }
        let look = &self.look;
        let this = self.this.clone();
        div().child(
            div()
                .id(("ticket-reply", thread.unsigned_abs()))
                .debug_selector(move || format!("ticket-reply-{thread}"))
                .text_size(look.small())
                .text_color(rgb(look.theme.muted))
                .cursor_pointer()
                .hover(|style| style.text_color(rgb(look.theme.foreground)))
                .child("Reply to the thread…")
                .on_click(move |_, window, cx| {
                    let _ =
                        this.update(cx, |this, cx| this.begin_comment(Some(thread), window, cx));
                }),
        )
    }

    fn new_thread(&self) -> Div {
        if self.composer.as_ref().map(|(answers, _)| *answers) == Some(None) {
            return self.composer();
        }
        let this = self.this.clone();
        div().child(
            button(&self.look, "ticket-comment-new", "Add a comment").on_click(
                move |_, window, cx| {
                    let _ = this.update(cx, |this, cx| this.begin_comment(None, window, cx));
                },
            ),
        )
    }

    /// The field a comment is written in, with Send and Cancel under it.
    fn composer(&self) -> Div {
        let Some((_, input)) = &self.composer else {
            return div();
        };
        let look = &self.look;
        let send = self.this.clone();
        let cancel = self.this.clone();
        div()
            .debug_selector(|| "ticket-composer".into())
            .flex()
            .flex_col()
            .gap_2()
            .child(input.clone())
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(look.muted("\u{2318}\u{21b5} to send, Esc to cancel"))
                    .child(div().flex_1())
                    .child(button(look, "ticket-comment-cancel", "Cancel").on_click(
                        move |_, window, cx| {
                            let _ = cancel.update(cx, |this, cx| this.cancel_comment(window, cx));
                        },
                    ))
                    .child(
                        button(
                            look,
                            "ticket-comment-send",
                            if self.sending { "Sending…" } else { "Send" },
                        )
                        .on_click(move |_, window, cx| {
                            let _ = send.update(cx, |this, cx| this.send_comment(window, cx));
                        }),
                    ),
            )
    }
}

/// A titled part of the page's main column.
fn section(look: &Look, title: impl Into<SharedString>) -> Div {
    div()
        .pt_4()
        .flex()
        .flex_col()
        .gap_2()
        .child(div().font_weight(FontWeight::SEMIBOLD).child(title.into()))
        .text_size(look.size())
}

fn comment_card(comment: &Comment, blocks: &[Markdown], look: &Look, cx: &App) -> Div {
    let when = chrono::DateTime::from_timestamp_millis(comment.created_at as i64)
        .map(|at| {
            at.with_timezone(&chrono::Local)
                .format("%b %-d, %H:%M")
                .to_string()
        })
        .unwrap_or_default();
    div()
        .when(comment.reply_to.is_some(), |card| card.ml_6())
        .p_3()
        .flex()
        .flex_col()
        .gap_2()
        .rounded(px(corners::CONTROL))
        .border_1()
        .border_color(rgb(look.theme.active))
        .child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .child(
                    div()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(comment.author.name().to_owned()),
                )
                .child(look.muted(when)),
        )
        .child(markdown_view::render(
            format!("ticket-comment-{}", comment.id),
            blocks,
            look,
            cx,
        ))
}
