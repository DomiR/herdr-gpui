//! How one line of a document is drawn: its size, and a text run for each
//! stretch of the same style. Markdown's markers keep their place in the
//! text; they are drawn muted, or not at all where a checkbox stands in for
//! them, so every byte keeps the place the caret and selection give it.

use crate::config::{FontConfig, Theme};
use gpui::{
    FontStyle, FontWeight, Hsla, SharedString, StrikethroughStyle, TextRun, UnderlineStyle, px, rgb,
};
use herdr_doc::markdown::{self, Kind, Line, Style};
use std::ops::Range;

/// What the editor draws with.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Look {
    pub(crate) theme: Theme,
    pub(crate) ui: FontConfig,
    pub(crate) code: FontConfig,
}

/// A line, ready to draw.
#[derive(Clone, Debug)]
pub(super) struct Row {
    pub(super) text: SharedString,
    pub(super) runs: Vec<TextRun>,
    pub(super) size: f32,
    pub(super) line_height: f32,
    /// A task's `[ ]`, in the line's own offsets, and whether it is done.
    pub(super) checkbox: Option<(Range<usize>, bool)>,
    /// Links, in the line's own offsets, and where they point.
    pub(super) links: Vec<(Range<usize>, String)>,
    pub(super) kind: Kind,
}

/// How much larger than body text each heading level draws.
fn heading_scale(level: u8) -> f32 {
    match level {
        1 => 1.6,
        2 => 1.35,
        3 => 1.18,
        _ => 1.05,
    }
}

/// One byte range's style, before it becomes a run.
#[derive(Clone, Copy, Default, PartialEq)]
struct Paint {
    bold: bool,
    italic: bool,
    code: bool,
    strike: bool,
    link: bool,
    muted: bool,
    hidden: bool,
    /// Inside a code block, whose row draws the background.
    block: bool,
}

/// `line` of `text`, styled.
pub(super) fn row(text: &str, line: &Line, look: &Look) -> Row {
    let source = &text[line.range.clone()];
    let base_start = line.range.start;
    let local = |range: &Range<usize>| {
        range.start.saturating_sub(base_start).min(source.len())
            ..range.end.saturating_sub(base_start).min(source.len())
    };
    let ui_size = look.ui.size.max(8.);
    let (size, line_height) = match line.kind {
        Kind::Heading { level } => {
            let size = ui_size * heading_scale(level);
            (size, size * 1.45)
        }
        Kind::Code | Kind::Fence => (look.code.size.max(8.), look.code.line_height()),
        _ => (ui_size, ui_size * 1.6),
    };
    let mut paints = vec![Paint::default(); source.len()];
    let mut mark = |range: Range<usize>, f: &dyn Fn(&mut Paint)| {
        for paint in &mut paints[range] {
            f(paint);
        }
    };
    let whole = 0..source.len();
    match line.kind {
        Kind::Heading { .. } => mark(whole.clone(), &|p| p.bold = true),
        Kind::Code | Kind::Fence => mark(whole.clone(), &|p| {
            p.code = true;
            p.block = true;
        }),
        Kind::Quote => mark(whole.clone(), &|p| {
            p.italic = true;
            p.muted = true;
        }),
        _ => {}
    }
    if line.kind == Kind::Fence {
        mark(whole.clone(), &|p| p.muted = true);
    }
    if !line.marker.is_empty() {
        mark(local(&line.marker), &|p| p.muted = true);
    }
    let checkbox = line
        .checkbox
        .as_ref()
        .map(|range| (local(range), line.task() == Some(true)));
    if let Some((range, done)) = &checkbox {
        mark(range.clone(), &|p| p.hidden = true);
        if *done {
            mark(local(&line.content()), &|p| {
                p.strike = true;
                p.muted = true;
            });
        }
    }
    let mut links = Vec::new();
    for span in markdown::inline(text, line) {
        let range = local(&span.range);
        match &span.style {
            Style::Strong => mark(range, &|p| p.bold = true),
            Style::Emphasis => mark(range, &|p| p.italic = true),
            Style::Code => mark(range, &|p| p.code = true),
            Style::Strike => mark(range, &|p| p.strike = true),
            Style::Marker => mark(range, &|p| p.muted = true),
            Style::Link { .. } => {
                if let Some(url) = markdown::link_url(text, &span) {
                    links.push((range.clone(), url.to_owned()));
                }
                mark(range, &|p| p.link = true);
            }
        }
    }
    let runs = runs(source, &paints, look);
    Row {
        text: source.to_owned().into(),
        runs,
        size,
        line_height,
        checkbox,
        links,
        kind: line.kind,
    }
}

/// One run per stretch of equal paint, never splitting a character.
fn runs(source: &str, paints: &[Paint], look: &Look) -> Vec<TextRun> {
    let mut runs: Vec<TextRun> = Vec::new();
    let mut start = 0;
    let boundaries = source
        .char_indices()
        .map(|(at, _)| at)
        .skip(1)
        .chain([source.len()]);
    let mut current = paints.first().copied().unwrap_or_default();
    for end in boundaries {
        let next = paints.get(end).copied().unwrap_or_default();
        if end == source.len() || next != current {
            runs.push(run(end - start, current, look));
            start = end;
            current = next;
        }
    }
    runs
}

fn run(len: usize, paint: Paint, look: &Look) -> TextRun {
    let theme = &look.theme;
    let mut font = if paint.code {
        look.code.font()
    } else {
        look.ui.font()
    };
    if paint.bold {
        font.weight = FontWeight::BOLD;
    }
    if paint.italic {
        font.style = FontStyle::Italic;
    }
    let color: Hsla = if paint.hidden {
        Hsla::transparent_black()
    } else if paint.link {
        rgb(theme.primary()).into()
    } else if paint.muted {
        rgb(theme.muted).into()
    } else {
        rgb(theme.foreground).into()
    };
    TextRun {
        len,
        font,
        color,
        background_color: (paint.code && !paint.block && !paint.hidden)
            .then(|| rgb(theme.surface).into()),
        underline: paint.link.then(|| UnderlineStyle {
            color: Some(color),
            thickness: px(1.),
            wavy: false,
        }),
        strikethrough: (paint.strike && !paint.hidden).then(|| StrikethroughStyle {
            thickness: px(1.),
            color: Some(color),
        }),
    }
}

#[cfg(test)]
mod tests;
