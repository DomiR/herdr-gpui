//! How one line of a document is drawn: its size, and a text run for each
//! stretch of the same style. Markdown's markers keep their place in the
//! text; they are drawn muted, or not at all where a checkbox stands in for
//! them, so every byte keeps the place the caret and selection give it.

use crate::config::{FontConfig, Theme};
use gpui::{
    FontStyle, FontWeight, Hsla, SharedString, StrikethroughStyle, TextRun, UnderlineStyle, px, rgb,
};
use sheprd_doc::markdown::{self, Kind, Line, Style, TagKind, TaskState};
use std::{ffi::OsStr, ops::Range};

/// What the editor draws with.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Look {
    pub(crate) theme: Theme,
    pub(super) inks: Option<super::theme::Inks>,
    pub(crate) ui: FontConfig,
    pub(crate) code: FontConfig,
}

/// Where a drawn line's text differs from the source line: each piece of
/// the source drawn as other text (or none), ascending.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct DisplayMap {
    pieces: Vec<Piece>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Piece {
    source: Range<usize>,
    display: Range<usize>,
}

impl DisplayMap {
    /// Where source offset `at` (in the line) is drawn. An offset inside a
    /// replaced piece is drawn at the replacement's start.
    pub(crate) fn to_display(&self, at: usize) -> usize {
        let mut shift: isize = 0;
        for piece in &self.pieces {
            if at < piece.source.start {
                break;
            }
            if at < piece.source.end {
                return piece.display.start;
            }
            shift = piece.display.end as isize - piece.source.end as isize;
        }
        at.saturating_add_signed(shift)
    }

    /// The source offset drawn at display offset `at`. Inside a replacement
    /// it is the piece's start, or its end from the replacement's end on;
    /// where hidden text was, it is after the hidden text.
    pub(crate) fn to_source(&self, at: usize) -> usize {
        let mut shift: isize = 0;
        for piece in &self.pieces {
            if at < piece.display.start {
                break;
            }
            if at < piece.display.end {
                return if at == piece.display.start {
                    piece.source.start
                } else {
                    piece.source.end
                };
            }
            shift = piece.source.end as isize - piece.display.end as isize;
        }
        at.saturating_add_signed(shift)
    }
}

/// Where a link goes: an address, or a file a code span names.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Target {
    Url(String),
    File(super::links::FileLink),
}

impl Target {
    pub(super) fn opens(&self) -> bool {
        match self {
            Self::Url(url) => web(url),
            Self::File(_) => true,
        }
    }
}

pub(super) fn web(url: &str) -> bool {
    crate::link::is_link(OsStr::new(url))
        || ["http://", "https://", "mailto:"]
            .iter()
            .any(|scheme| url.starts_with(scheme))
}

/// A line, ready to draw.
#[derive(Clone, Debug)]
pub(super) struct Row {
    /// What is drawn: the source line, with only a task bullet hidden at the caret.
    pub(super) text: SharedString,
    /// How the drawn text maps to the source line.
    pub(super) map: std::rc::Rc<DisplayMap>,
    pub(super) runs: Vec<TextRun>,
    pub(super) size: f32,
    pub(super) line_height: f32,
    /// A task's checkbox, in the drawn text's offsets, and its state.
    pub(super) checkbox: Option<(Range<usize>, TaskState)>,
    /// Links, in the source line's offsets, and where they point.
    pub(super) links: Vec<(Range<usize>, Target)>,
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
    /// Plain document prose, dimmed below subtext.
    prose: bool,
    hidden: bool,
    /// Inside a code block, whose row draws the background.
    block: bool,
    /// A code token's colour, inside a coloured code block.
    token: Option<crate::review::highlight::Token>,
    /// Notebook-only roles, left unset for the App run boundaries.
    pending: bool,
    done: bool,
    delimiter: bool,
    cancelled: bool,
    tag: Option<TagKind>,
    tag_meta: bool,
    notes: bool,
    separator: bool,
}

/// `line` of `text`, styled; `decorate` hides its markers, as for a line
/// away from the caret. An editing row hides only a bulleted task's `- `.
/// `code` colours a code block line.
pub(super) fn row(
    text: &str,
    line: &Line,
    look: &Look,
    decorate: bool,
    code: &[crate::review::highlight::Span],
    files: &dyn for<'a> Fn(super::links::Key<'a>) -> Option<super::links::FileLink>,
) -> Row {
    let source = &text[line.range.clone()];
    let base_start = line.range.start;
    let local = |range: &Range<usize>| {
        range.start.saturating_sub(base_start).min(source.len())
            ..range.end.saturating_sub(base_start).min(source.len())
    };
    let ui_size = look.ui.size.max(8.);
    let (size, line_height) = match line.kind {
        Kind::Heading { level } if look.inks.is_none() => {
            let size = ui_size * heading_scale(level);
            (size, size * 1.45)
        }
        Kind::Heading { .. } => (ui_size, ui_size * 1.6),
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
        Kind::Heading { .. } if look.inks.is_none() => mark(whole.clone(), &|p| p.bold = true),
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
    if look.inks.is_some() {
        match line.kind {
            Kind::Fence | Kind::Rule => mark(whole.clone(), &|p| p.separator = true),
            Kind::Quote | Kind::Item { task: Some(_), .. } => {
                mark(local(&line.marker), &|p| p.separator = true)
            }
            Kind::Item { task: None, .. } => mark(whole.clone(), &|p| p.notes = true),
            _ => {}
        }
        if line.kind == Kind::Quote {
            mark(local(&line.content()), &|p| p.notes = true);
        }
        if let Some(state) = line.task() {
            mark(local(&line.content()), &|p| {
                p.pending = state == TaskState::Open;
                p.done = state == TaskState::Done;
                p.cancelled = state == TaskState::Cancelled;
            });
        }
    }
    match line.kind {
        Kind::Paragraph | Kind::Item { task: None, .. } => {
            mark(local(&line.content()), &|p| p.prose = true)
        }
        _ => {}
    }
    for span in code {
        let range = span.start.min(source.len())..span.end.min(source.len());
        let token = span.token;
        mark(range, &|p| p.token = Some(token));
    }
    if !line.marker.is_empty() {
        mark(local(&line.marker), &|p| p.muted = true);
    }
    let checkbox = line
        .task()
        .and_then(|state| line.checkbox.as_ref().map(|range| (local(range), state)));
    if let Some((range, state)) = &checkbox {
        mark(range.clone(), &|p| p.hidden = true);
        if look.inks.is_some() {
            if state.is_done() {
                mark(local(&line.content()), &|p| p.strike = true);
            }
        } else if state.is_resolved() {
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
            Style::Strong => mark(range, &|p| {
                p.bold = true;
                p.prose = false;
            }),
            Style::Emphasis => mark(range, &|p| {
                p.italic = true;
                p.prose = false;
            }),
            Style::Code => {
                if let Some(file) = text.get(span.range.clone()).and_then(|path| {
                    files(super::links::Key {
                        form: super::links::Form::Code,
                        text: path,
                    })
                }) {
                    links.push((range.clone(), Target::File(file)));
                    mark(range.clone(), &|p| {
                        p.link = true;
                        p.prose = false;
                    });
                }
                mark(range, &|p| {
                    p.code = true;
                    p.prose = false;
                });
            }
            Style::Strike => mark(range, &|p| p.strike = true),
            Style::Marker => mark(range, &|p| {
                p.muted = true;
                p.delimiter = look.inks.is_some();
            }),
            Style::Link { .. } => {
                if let Some(url) = markdown::link_url(text, &span) {
                    if web(url) {
                        links.push((range.clone(), Target::Url(url.to_owned())));
                    } else if let Some(target) = super::links::target(text, &span)
                        && let Some(file) = files(super::links::Key {
                            form: super::links::Form::Target,
                            text: target,
                        })
                    {
                        links.push((range.clone(), Target::File(file)));
                    }
                }
                mark(range, &|p| {
                    p.link = true;
                    p.prose = false;
                });
            }
        }
    }
    for range in super::links::bare_paths(text, line) {
        let Some(path) = text.get(range.clone()) else {
            continue;
        };
        if let Some(file) = files(super::links::Key {
            form: super::links::Form::Prose,
            text: path,
        }) {
            let local_range = local(&range);
            links.push((local_range.clone(), Target::File(file)));
            mark(local_range, &|p| {
                p.link = true;
                p.prose = false;
            });
        }
    }
    links.sort_by_key(|(range, _)| range.start);
    // Last, so a tag stays unstruck and upright inside a done task or an
    // emphasis span, as PlainTasks draws it.
    if look.inks.is_some() {
        let tag_meta = line.task().is_some_and(TaskState::is_resolved);
        for tag in markdown::tags(text, line) {
            mark(local(&tag.range), &|p| {
                p.tag = Some(tag.kind);
                p.tag_meta = tag_meta;
                p.bold = !tag_meta && tag.kind != TagKind::Cancelled;
                p.italic = false;
                p.strike = false;
            });
        }
    }
    let decorations = if decorate {
    let decorations = markdown::decorations(text, line);
    if look.inks.is_some() && line.is_heading() {
        decorations
            .into_iter()
            .filter(|decoration| decoration.range != line.marker)
            .collect()
    } else {
        decorations
    }
} else {
    markdown::editing_decorations(line)
};
    let mut drawn = String::with_capacity(source.len());
    let mut drawn_paints = Vec::with_capacity(source.len());
    let mut pieces = Vec::new();
    let mut at = 0;
    for decoration in decorations {
        let range = local(&decoration.range);
        if range.start < at {
            continue;
        }
        drawn.push_str(&source[at..range.start]);
        drawn_paints.extend_from_slice(&paints[at..range.start]);
        let start = drawn.len();
        drawn.push_str(decoration.with);
        let paint = paints.get(range.start).copied().unwrap_or_default();
        drawn_paints.extend(std::iter::repeat_n(paint, decoration.with.len()));
        pieces.push(Piece {
            source: range.clone(),
            display: start..drawn.len(),
        });
        at = range.end;
    }
    drawn.push_str(&source[at..]);
    drawn_paints.extend_from_slice(&paints[at..]);
    let map = DisplayMap { pieces };
    let checkbox = checkbox.map(|(range, state)| {
        (
            map.to_display(range.start)..map.to_display(range.end),
            state,
        )
    });
    let runs = runs(&drawn, &drawn_paints, look);
    Row {
        text: drawn.into(),
        map: std::rc::Rc::new(map),
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
    if paint.bold && !paint.tag_meta {
        font.weight = FontWeight::BOLD;
    }
    let plain_tasks_body = paint.pending || paint.done || paint.cancelled;
    if paint.italic
        || (look.inks.is_some()
            && plain_tasks_body
            && paint.tag.is_none()
            && !paint.link
            && !paint.tag_meta)
    {
        font.style = FontStyle::Italic;
    }
    if look.inks.is_some() && paint.link && !paint.tag_meta {
        font.weight = FontWeight::BOLD;
    }
    let color: Hsla = if let Some(inks) = look.inks {
        if paint.hidden {
            Hsla::transparent_black()
        } else if let Some(token) = paint.token {
            rgb(token.colour(theme, crate::review::highlight::Contrast::On(theme.surface))).into()
        } else if paint.tag_meta {
            inks.tag_meta
        } else if let Some(tag) = paint.tag {
            match tag {
                TagKind::Cancelled => inks.tag_meta,
                TagKind::Critical => inks.critical.0,
                TagKind::Done => inks.tag_done,
                TagKind::Generic => inks.tag,
                TagKind::High => inks.high.0,
                TagKind::Low => inks.low.0,
                TagKind::Today => inks.today.0,
            }
        } else if paint.separator {
            inks.separator
        } else if paint.delimiter {
            inks.delimiter
        } else if paint.link {
            inks.url
        } else if paint.done {
            inks.done
        } else if paint.cancelled {
            inks.cancelled
        } else if paint.pending {
            inks.pending
        } else if paint.notes {
            inks.notes
        } else {
            inks.plain
        }
    } else if paint.hidden {
        Hsla::transparent_black()
    } else if let Some(token) = paint.token {
        rgb(token.colour(theme, crate::review::highlight::Contrast::On(theme.surface))).into()
    } else if paint.link {
        rgb(theme.primary()).into()
    } else if paint.muted {
        rgb(theme.muted).into()
    } else if paint.prose {
        rgb(theme.prose()).into()
    } else {
        rgb(theme.foreground).into()
    };
    let tag_background = if let Some(inks) = look.inks
        && let Some(tag) = paint.tag
        && !paint.tag_meta
        && !paint.hidden
    {
        match tag {
            TagKind::Cancelled | TagKind::Done => None,
            TagKind::Today => Some(inks.today.1),
            TagKind::High => Some(inks.high.1),
            TagKind::Critical => Some(inks.critical.1),
            TagKind::Low => Some(inks.low.1),
            TagKind::Generic => None,
        }
    } else {
        None
    };
    TextRun {
        len,
        font,
        color,
        background_color: tag_background.or_else(|| {
            (paint.code && !paint.block && !paint.hidden).then(|| rgb(theme.surface).into())
        }),
        underline: paint.link.then(|| UnderlineStyle {
            color: Some(color),
            thickness: px(1.),
            wavy: false,
        }),
        strikethrough: (paint.strike && !paint.hidden).then(|| StrikethroughStyle {
            thickness: px(1.),
            color: Some(
                if let Some(inks) = look.inks
                    && paint.done
                {
                    inks.done
                } else {
                    color
                },
            ),
        }),
    }
}

#[cfg(test)]
mod tests;
