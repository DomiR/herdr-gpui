//! Document-local Notebook colours; the surrounding app keeps its theme.

use crate::config::{DocumentTheme, Theme};
use gpui::{Hsla, rgba};

// Adapted from PlainTasks tasks.hidden-tmTheme (Notebook) at
// 0b299a78b82a070ed8ef6e09c4e9c0e7b6403d68, MIT. Marker shapes follow the
// classic bullets in PlainTasks.sublime-settings at
// 138f0bebd1de0e956396be27b0f549e190425518, MIT. See the root NOTICE.
const PAPER: u32 = 0xefe9b7;
const PROSE: u32 = 0x858266;
const BLACK: u32 = 0x000000;
const DONE: u32 = 0x00000090;
const HEADING: u32 = 0xbc644aff;
const HEADING_BACKGROUND: u32 = 0x00000010;
const LINK: u32 = 0x003e7290;
// The source's delimiter ink (#ded9a6) is too faint on the paper. Use its
// separator ink for visible Markdown delimiters, quotes and fences too.
const SEPARATOR: u32 = 0x00000044;
const SELECTION: u32 = 0x00000029;
const CARET_ROW: u32 = 0xf7dc8688;
const OPEN_MARKER: u32 = 0x00000066;
const DONE_MARKER: u32 = 0x00723eff;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Inks {
    pub(super) prose: Hsla,
    pub(super) pending: Hsla,
    pub(super) done: Hsla,
    pub(super) heading: Hsla,
    pub(super) heading_background: Hsla,
    pub(super) delimiter: Hsla,
    pub(super) link: Hsla,
    pub(super) separator: Hsla,
    pub(super) selection: Hsla,
    pub(super) caret: Hsla,
    pub(super) caret_row: Hsla,
    pub(super) open_marker: Hsla,
    pub(super) done_marker: Hsla,
}

/// Alpha roles stay translucent; only Theme's opaque fields are composited.
pub(super) fn resolve(choice: DocumentTheme, app: &Theme) -> (Theme, Option<Inks>) {
    if choice == DocumentTheme::App {
        return (app.clone(), None);
    }
    let mut theme = app.clone();
    theme.background = PAPER;
    theme.foreground = BLACK;
    theme.cursor = BLACK;
    theme.muted = PROSE;
    theme.surface = composite(PAPER, BLACK, 0x10);
    theme.active = composite(PAPER, BLACK, 0x29);
    theme.accent = Some(composite(PAPER, LINK >> 8, 0x90));
    theme.system_accent = None;
    let inks = Inks {
        prose: rgba((PROSE << 8) | 0xff).into(),
        pending: rgba((BLACK << 8) | 0xff).into(),
        done: rgba(DONE).into(),
        heading: rgba(HEADING).into(),
        heading_background: rgba(HEADING_BACKGROUND).into(),
        delimiter: rgba(SEPARATOR).into(),
        link: rgba(LINK).into(),
        separator: rgba(SEPARATOR).into(),
        selection: rgba(SELECTION).into(),
        caret: rgba((BLACK << 8) | 0xff).into(),
        caret_row: rgba(CARET_ROW).into(),
        open_marker: rgba(OPEN_MARKER).into(),
        done_marker: rgba(DONE_MARKER).into(),
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
