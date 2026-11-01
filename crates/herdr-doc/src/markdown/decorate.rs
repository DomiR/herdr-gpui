//! What a line looks like away from the caret, as Obsidian's live preview
//! and comet's composer draw it: heading and quote markers, emphasis and
//! code delimiters, and a link's brackets and address hidden, bullets drawn
//! as `•`. Only drawing changes; each replacement keeps its source range,
//! so a click or selection on the drawn line maps back to the text.

use super::{Kind, Line, List, Style, inline};
use std::ops::Range;

/// One replacement: `range` of the text is drawn as `with`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Decoration {
    pub range: Range<usize>,
    pub with: &'static str,
}

/// The replacements for `line` of `text`, ascending and not overlapping.
pub fn decorations(text: &str, line: &Line) -> Vec<Decoration> {
    let mut out = Vec::new();
    let hide = |range: Range<usize>| Decoration { range, with: "" };
    match line.kind {
        Kind::Heading { .. } | Kind::Quote if !line.marker.is_empty() => {
            out.push(hide(line.marker.clone()));
        }
        Kind::Item {
            list: List::Bullet(_),
            task,
        } => {
            let bullet = line.marker.start..line.marker.start + 1;
            match task {
                // The checkbox stands in for the bullet.
                Some(_) => out.push(hide(line.marker.start..line.marker.start + 2)),
                None => out.push(Decoration {
                    range: bullet,
                    with: "•",
                }),
            }
        }
        _ => {}
    }
    for span in inline(text, line) {
        if span.style == Style::Marker && !span.range.is_empty() {
            out.push(hide(span.range));
        }
    }
    out.sort_by_key(|decoration| decoration.range.start);
    // Spans of nested styles may touch or repeat; keep the first of any
    // overlapping pair.
    let mut kept: Vec<Decoration> = Vec::with_capacity(out.len());
    for decoration in out {
        if kept
            .last()
            .is_some_and(|last| decoration.range.start < last.range.end)
        {
            continue;
        }
        kept.push(decoration);
    }
    kept
}
