//! Document-local Notebook colours; the surrounding app keeps its theme.

use crate::config::{DocumentTheme, Theme};
use gpui::{Hsla, rgba};

// Adapted from PlainTasks tasks.hidden-tmTheme (Notebook) at
// 0b299a78b82a070ed8ef6e09c4e9c0e7b6403d68, MIT. Marker shapes follow the
// classic bullets in PlainTasks.sublime-settings at
// 138f0bebd1de0e956396be27b0f549e190425518, MIT. See the root NOTICE.
// settings[0].background
const PAPER: u32 = 0xefe9b7;
// settings[0].foreground
const DEFAULT: u32 = 0x00000050;
// notes.todo
const NOTES: u32 = 0x858266;
// meta.item.todo.pending
const PENDING: u32 = 0x000000;
// comment
const DONE: u32 = 0x00000090;
// meta.item.todo.cancelled
const CANCELLED: u32 = 0x68654f;
// punctuation.definition.bullet.pending
const OPEN_MARKER: u32 = 0x00000066;
// punctuation.definition.bullet.completed
const DONE_MARKER: u32 = 0x00723e;
// punctuation.definition.bullet.cancelled
const CANCELLED_MARKER: u32 = 0xb60101;
// meta.tag.todo
const TAG: u32 = 0xa02e2b99;
// meta.tag.todo.completed and meta.tag.todo.cancelled
const TAG_META: u32 = 0x00000050;
// todo.url
const URL: u32 = 0x003e7290;
// punctuation.definition.bold and punctuation.definition.italic
const DELIMITER: u32 = 0xded9a6;
// meta.punctuation.separator.todo
const SEPARATOR: u32 = 0x00000044;
// settings[0].selection
const SELECTION: u32 = 0x00000029;
// settings[0].caret
const CARET: u32 = 0x000000;
// settings[0].lineHighlight
const CARET_ROW: u32 = 0xf7dc8688;
// string.other.tag.todo.today
const TODAY: u32 = 0x000000;
// string.other.tag.todo.today
const TODAY_BACKGROUND: u32 = 0xeadd4e;
// string.other.tag.todo.high
const HIGH: u32 = 0x000000;
// string.other.tag.todo.high
const HIGH_BACKGROUND: u32 = 0xff7f00;
// string.other.tag.todo.critical
const CRITICAL: u32 = 0x000000;
// string.other.tag.todo.critical
const CRITICAL_BACKGROUND: u32 = 0xff0000;
// string.other.tag.todo.low
const LOW: u32 = 0xffffff;
// string.other.tag.todo.low
const LOW_BACKGROUND: u32 = 0x222222;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Inks {
    pub(super) plain: Hsla,
    pub(super) notes: Hsla,
    pub(super) pending: Hsla,
    pub(super) done: Hsla,
    pub(super) cancelled: Hsla,
    pub(super) delimiter: Hsla,
    pub(super) tag: Hsla,
    pub(super) tag_meta: Hsla,
    pub(super) url: Hsla,
    pub(super) today: (Hsla, Hsla),
    pub(super) high: (Hsla, Hsla),
    pub(super) critical: (Hsla, Hsla),
    pub(super) low: (Hsla, Hsla),
    pub(super) separator: Hsla,
    pub(super) selection: Hsla,
    pub(super) caret: Hsla,
    pub(super) caret_row: Hsla,
    pub(super) open_marker: Hsla,
    pub(super) done_marker: Hsla,
    pub(super) cancelled_marker: Hsla,
}

/// Alpha roles stay translucent; only Theme's opaque fields are composited.
pub(super) fn resolve(choice: DocumentTheme, app: &Theme) -> (Theme, Option<Inks>) {
    if choice == DocumentTheme::App {
        return (app.clone(), None);
    }
    let mut theme = app.clone();
    theme.background = PAPER;
    theme.foreground = PENDING;
    theme.cursor = PENDING;
    theme.muted = NOTES;
    theme.surface = composite(PAPER, PENDING, 0x10);
    theme.active = composite(PAPER, PENDING, 0x29);
    theme.accent = Some(composite(PAPER, URL >> 8, 0x90));
    theme.system_accent = None;
    let inks = Inks {
        plain: rgba(DEFAULT).into(),
        notes: rgba((NOTES << 8) | 0xff).into(),
        pending: rgba((PENDING << 8) | 0xff).into(),
        done: rgba(DONE).into(),
        cancelled: rgba((CANCELLED << 8) | 0xff).into(),
        delimiter: rgba((DELIMITER << 8) | 0xff).into(),
        tag: rgba(TAG).into(),
        tag_meta: rgba(TAG_META).into(),
        url: rgba(URL).into(),
        today: (
            rgba((TODAY << 8) | 0xff).into(),
            rgba((TODAY_BACKGROUND << 8) | 0xff).into(),
        ),
        high: (
            rgba((HIGH << 8) | 0xff).into(),
            rgba((HIGH_BACKGROUND << 8) | 0xff).into(),
        ),
        critical: (
            rgba((CRITICAL << 8) | 0xff).into(),
            rgba((CRITICAL_BACKGROUND << 8) | 0xff).into(),
        ),
        low: (
            rgba((LOW << 8) | 0xff).into(),
            rgba((LOW_BACKGROUND << 8) | 0xff).into(),
        ),
        separator: rgba(SEPARATOR).into(),
        selection: rgba(SELECTION).into(),
        caret: rgba((CARET << 8) | 0xff).into(),
        caret_row: rgba(CARET_ROW).into(),
        open_marker: rgba(OPEN_MARKER).into(),
        done_marker: rgba((DONE_MARKER << 8) | 0xff).into(),
        cancelled_marker: rgba((CANCELLED_MARKER << 8) | 0xff).into(),
    };
    (theme, Some(inks))
}

/// Source-over blending of two packed RGB colours, rounded to the nearest byte.
fn composite(paper: u32, ink: u32, alpha: u8) -> u32 {
    let alpha = u32::from(alpha);
    let channel = |shift: u32| {
        let paper = (paper >> shift) & 255;
        let ink = (ink >> shift) & 255;
        (paper * (255 - alpha) + ink * alpha + 127) / 255
    };
    (channel(16) << 16) | (channel(8) << 8) | channel(0)
}

#[cfg(test)]
mod tests;
