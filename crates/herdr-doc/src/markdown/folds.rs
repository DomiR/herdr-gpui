//! What can fold: a heading's section, a list item's sub-items, and a
//! fenced code block's body. A fold hides whole lines after the one that
//! holds it, never that line, so a folded heading still shows its title.

use super::{Kind, Line};
use std::ops::Range;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FoldKind {
    Section,
    Item,
    Code,
}

/// One foldable region: `line` holds it, `hidden` are the lines it hides.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Fold {
    pub line: usize,
    pub hidden: Range<usize>,
    pub kind: FoldKind,
}

/// Every fold of `lines`, by the line that holds it, ascending. Trailing
/// blank lines stay outside a fold, so a folded section keeps its gap
/// before the next heading.
pub fn folds(lines: &[Line]) -> Vec<Fold> {
    let mut folds = Vec::new();
    // Fence lines alternate: an opening one, then its closing one.
    let mut in_code = false;
    for (index, line) in lines.iter().enumerate() {
        let opens = line.kind == Kind::Fence && !in_code;
        if line.kind == Kind::Fence {
            in_code = !in_code;
        }
        let end = match line.kind {
            Kind::Heading { level } => {
                let next = lines[index + 1..]
                    .iter()
                    .position(|other| other.level().is_some_and(|other| other <= level))
                    .map_or(lines.len(), |at| index + 1 + at);
                trim_blank(lines, index + 1, next)
            }
            Kind::Item { .. } => {
                let next = lines[index + 1..]
                    .iter()
                    .position(|other| {
                        other.kind != Kind::Blank
                            && (other.depth <= line.depth || other.is_heading())
                            && other.kind != Kind::Code
                    })
                    .map_or(lines.len(), |at| index + 1 + at);
                trim_blank(lines, index + 1, next)
            }
            Kind::Fence if opens => lines[index + 1..]
                .iter()
                .position(|other| other.kind == Kind::Fence)
                .map_or(lines.len(), |at| index + 2 + at),
            _ => continue,
        };
        // A code block's closing fence alone is not worth folding.
        let least = if line.kind == Kind::Fence { 2 } else { 1 };
        if end > index + least {
            folds.push(Fold {
                line: index,
                hidden: index + 1..end,
                kind: match line.kind {
                    Kind::Heading { .. } => FoldKind::Section,
                    Kind::Item { .. } => FoldKind::Item,
                    _ => FoldKind::Code,
                },
            });
        }
    }
    folds
}

fn trim_blank(lines: &[Line], start: usize, mut end: usize) -> usize {
    while end > start && lines[end - 1].kind == Kind::Blank {
        end -= 1;
    }
    end
}
