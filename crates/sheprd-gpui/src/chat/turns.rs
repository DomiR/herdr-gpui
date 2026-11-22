//! The turns of a chat for a host's layout, which draws them as a ruler in
//! the chat's left gutter (Moonframe's revision timeline): one tick per
//! prompt, the reply's start on its card, and a click that scrolls the
//! conversation back to that prompt.
//!
//! Where the gutter is comes from layout, not from the pane's place: each
//! chat drawn while a host lays out the window records its entries' box and
//! the row at their top as it paints ([`SheprdWindow::chat_turns_probe`]),
//! and a sweep after every chat drops the ones not drawn this frame. Either
//! notifies the window when what it recorded changed, so the host follows a
//! resize, a scroll, or a chat going away in the next frame.

use super::chats::ChatPane;
use crate::{SheprdWindow, notifications::safe_text};
use gpui::{Bounds, Context, IntoElement, Pixels, Point, Styled as _, canvas};
use serde_json::{Value, json};
use sheprd_chat_protocol::{Entry, EntryId, EntryKind, assist::typed};
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    rc::Rc,
};

/// A gutter narrower than this shows no ruler: its card would not fit.
const MIN_GUTTER: f32 = 120.;
/// Characters of a prompt on its card.
const MAX_LABEL: usize = 80;
/// Characters of the reply on the card: about its three lines.
const MAX_SUMMARY: usize = 240;
/// Bytes of an entry's text read for the card, so a long paste costs no
/// more than a short one.
const MAX_READ: usize = 1024;
/// The most bytes of text the timeline takes in a label or summary is 512.
const MAX_BYTES: usize = 500;
/// The most ticks the ruler takes; earlier turns drop off the top.
const MAX_TURNS: usize = 10_000;
/// Points between ticks: a tight stack, as Moonframe's gallery draws the rail.
const SPACING: f32 = 14.;
/// The room the timeline keeps above and below its ticks.
const MARGINS: f32 = 40.;
/// The least height the timeline gets: its card under the pointer (three
/// lines of reply under the prompt) stays inside its box, which a few ticks
/// alone would not leave room for.
const MIN_HEIGHT: f32 = 96.;

/// Where each chat drawn last laid out its entries, by pane.
#[derive(Debug, Default)]
pub(crate) struct Spots {
    /// Bumped by every frame of the window, so the sweep knows what this
    /// frame drew.
    frame: Cell<u64>,
    /// The window's own origin, which the host's `main` region starts at.
    origin: Rc<Cell<Point<Pixels>>>,
    spots: Rc<RefCell<BTreeMap<String, Spot>>>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Spot {
    /// The entries' box, in window coordinates.
    area: Bounds<Pixels>,
    frame: u64,
    /// The row at the entries' top.
    top: usize,
}

/// One prompt, as its tick and card show it.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Turn {
    pub(crate) entry: EntryId,
    /// Its place among the entries, which the list's rows share.
    pub(crate) index: usize,
    pub(crate) label: String,
    /// The start of the reply, empty until one comes.
    pub(crate) summary: String,
}

/// What [`SheprdWindow::hosted_chat_turns`] is built from, cheap to compare
/// on every notification.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TurnsStamp {
    area: Bounds<Pixels>,
    following: bool,
    origin: Point<Pixels>,
    pane: String,
    revision: (usize, Option<(EntryId, u64)>),
    top: usize,
    width: u16,
}

/// The start of `text` on one running line, at most `max` characters and
/// [`MAX_BYTES`] bytes.
fn line(text: &str, max: usize) -> String {
    let text = text.trim_start();
    let start = &text[..text.floor_char_boundary(MAX_READ)];
    let mut line = safe_text(&start.split_whitespace().collect::<Vec<_>>().join(" "), max);
    line.truncate(line.floor_char_boundary(MAX_BYTES));
    line
}

/// Every prompt of `entries`, with the start of the reply that followed it.
pub(crate) fn turns(entries: &[Entry]) -> Vec<Turn> {
    let mut turns: Vec<Turn> = Vec::new();
    for (index, entry) in entries.iter().enumerate() {
        match &entry.kind {
            EntryKind::User { text, files, .. } => {
                // A prompt of attachments alone is named by them.
                let label = Some(line(typed(text, *files), MAX_LABEL))
                    .filter(|label| !label.is_empty())
                    .unwrap_or_else(|| line(text, MAX_LABEL));
                turns.push(Turn {
                    entry: entry.id,
                    index,
                    label,
                    summary: String::new(),
                });
            }
            EntryKind::Agent { text } => {
                if let Some(turn) = turns.last_mut().filter(|turn| turn.summary.is_empty()) {
                    turn.summary = line(text, MAX_SUMMARY);
                }
            }
            _ => {}
        }
    }
    let skip = turns.len().saturating_sub(MAX_TURNS);
    turns.drain(..skip);
    turns
}

/// The turn being read: the last while the list follows its end, else the
/// one whose prompt is at or above the top row.
pub(crate) fn reading(turns: &[Turn], top: usize, following: bool) -> Option<&Turn> {
    if following {
        return turns.last();
    }
    turns
        .iter()
        .rev()
        .find(|turn| turn.index <= top)
        .or_else(|| turns.first())
}

/// The stack of `count` ticks centered in a gutter `height` tall: its top
/// below the gutter's, and its height, never less than a card needs, and
/// the whole gutter once they fill it (the timeline then scrolls).
pub(crate) fn stack(count: usize, height: f32) -> (f32, f32) {
    let wanted = (count as f32 * SPACING + MARGINS)
        .max(MIN_HEIGHT)
        .min(height);
    ((height - wanted) / 2., wanted)
}

/// The gutter left of a reading column at most `max_width` wide (0: no
/// limit), centered in entries `width` wide.
pub(crate) fn gutter(width: f32, max_width: u16) -> f32 {
    if max_width == 0 {
        return 0.;
    }
    ((width - f32::from(max_width)) / 2.).max(0.)
}

impl Spots {
    /// A new frame of the window starts.
    pub(crate) fn next_frame(&self) {
        self.frame.set(self.frame.get().wrapping_add(1));
    }

    fn get(&self, pane: &str) -> Option<Spot> {
        self.spots.borrow().get(pane).copied()
    }

    /// The panes drawn last, in their order.
    fn panes(&self) -> Vec<String> {
        self.spots.borrow().keys().cloned().collect()
    }
}

impl SheprdWindow {
    /// Records where `chat`'s entries are as they paint, for a host's layout.
    pub(super) fn chat_turns_probe(
        &self,
        chat: &ChatPane,
        cx: &mut Context<Self>,
    ) -> Option<impl IntoElement + use<>> {
        if !self.hosted {
            return None;
        }
        let spots = self.chat_turns.spots.clone();
        let frame = self.chat_turns.frame.get();
        let pane = chat.target.pane_id.clone();
        let list = chat.list.clone();
        let view = cx.entity().downgrade();
        Some(
            canvas(
                move |area, _, cx| {
                    let spot = Spot {
                        area,
                        frame,
                        top: list.logical_scroll_top().item_ix,
                    };
                    let old = spots.borrow_mut().insert(pane, spot);
                    let moved = old.is_none_or(|old| old.area != area || old.top != spot.top);
                    if moved {
                        let _ = view.update(cx, |_, cx| cx.notify());
                    }
                },
                |_, _, _, _| {},
            )
            .absolute()
            .size_full(),
        )
    }

    /// After every chat: forgets those this frame did not draw, and records
    /// the window's origin.
    pub(crate) fn chat_turns_sweep(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let spots = self.chat_turns.spots.clone();
        let origin = self.chat_turns.origin.clone();
        let frame = self.chat_turns.frame.get();
        let view = cx.entity().downgrade();
        canvas(
            move |bounds, _, cx| {
                let before = spots.borrow().len();
                spots.borrow_mut().retain(|_, spot| spot.frame == frame);
                let moved = origin.replace(bounds.origin) != bounds.origin;
                if moved || spots.borrow().len() != before {
                    let _ = view.update(cx, |_, cx| cx.notify());
                }
            },
            |_, _, _, _| {},
        )
        .absolute()
        .size_full()
    }

    /// The chat whose turns a host shows: Herdr's focused pane when it is a
    /// chat on screen, else the first on screen.
    fn turns_pane(&self) -> Option<(String, Spot, &ChatPane)> {
        let focused = self
            .live
            .snapshot
            .as_ref()
            .and_then(|snapshot| snapshot.focused_pane_id.clone());
        let panes = self.chat_turns.panes();
        let pane = focused
            .filter(|pane| panes.contains(pane))
            .or_else(|| panes.into_iter().next())?;
        let spot = self.chat_turns.get(&pane)?;
        let chat = self.chats.pane(&pane).filter(|_| self.chats.shown(&pane))?;
        Some((pane, spot, chat))
    }

    /// Whether [`Self::hosted_chat_turns`] may have changed.
    pub(crate) fn hosted_chat_turns_stamp(&self) -> Option<TurnsStamp> {
        let (pane, spot, chat) = self.turns_pane()?;
        Some(TurnsStamp {
            area: spot.area,
            following: !chat.glide.stopped(),
            origin: self.chat_turns.origin.get(),
            pane,
            revision: chat.revision(),
            top: spot.top,
            width: self.config.chat.max_width,
        })
    }

    /// The ruler for a host's layout: where it goes, in the `main` region's
    /// coordinates, and the turns. Null while no chat shows, its gutter is
    /// too narrow, or it has fewer than two prompts to move between.
    pub(crate) fn hosted_chat_turns(&self) -> Value {
        let Some((pane, spot, chat)) = self.turns_pane() else {
            return Value::Null;
        };
        let width = gutter(f32::from(spot.area.size.width), self.config.chat.max_width);
        if width < MIN_GUTTER {
            return Value::Null;
        }
        let turns = turns(&chat.session.transcript().entries);
        if turns.len() < 2 {
            return Value::Null;
        }
        let selected = reading(&turns, spot.top, !chat.glide.stopped());
        let origin = self.chat_turns.origin.get();
        let (offset, height) = stack(turns.len(), f32::from(spot.area.size.height));
        let entries: Vec<Value> = turns
            .iter()
            .map(|turn| {
                // The timeline takes no empty text: a prompt still without
                // a reply has no summary, and one with no words a mark.
                json!({
                    "id": turn.entry.0.to_string(),
                    "label": if turn.label.is_empty() { "…" } else { turn.label.as_str() },
                    "summary": (!turn.summary.is_empty()).then_some(turn.summary.as_str()),
                })
            })
            .collect();
        json!({
            "pane": pane,
            "left": f32::from(spot.area.origin.x - origin.x),
            "top": (f32::from(spot.area.origin.y - origin.y) + offset).floor(),
            "width": width.floor(),
            "height": height.floor(),
            "spacing": SPACING,
            "selected": selected.map(|turn| turn.entry.0.to_string()),
            "entries": entries,
        })
    }

    /// Scrolls `pane`'s conversation to the prompt `entry`, if it still has it.
    pub(crate) fn go_to_chat_turn(&mut self, pane: &str, entry: u64, cx: &mut Context<Self>) {
        let index = self.chats.pane(pane).and_then(|chat| {
            chat.session
                .transcript()
                .entries
                .iter()
                .position(|candidate| candidate.id == EntryId(entry))
        });
        if let Some(index) = index {
            self.go_to_chat_prompt(pane, index, cx);
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests;
