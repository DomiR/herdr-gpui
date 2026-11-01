//! Which tasks changed between two texts, as one line each: what `doc
//! watch --todos` prints. Tasks are matched by their text in order, so a
//! task whose box flipped reads as checked or unchecked, one whose text
//! changed in place as edited, and the rest as added or removed.

use super::{Todo, todos};
use similar::{Algorithm, DiffOp, capture_diff_slices};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ChangeKind {
    Added,
    Removed,
    Checked,
    Unchecked,
    Edited,
}

impl ChangeKind {
    /// The word `doc watch --todos` prints.
    pub fn name(self) -> &'static str {
        match self {
            Self::Added => "added",
            Self::Removed => "removed",
            Self::Checked => "checked",
            Self::Unchecked => "unchecked",
            Self::Edited => "edited",
        }
    }
}

/// One task that changed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TodoChange {
    pub kind: ChangeKind,
    /// Its number in the new text (the old one, for a removed task).
    pub number: usize,
    pub text: String,
    /// Its 0-based line in the new text; for a removed task, `old_line`.
    pub line: usize,
    pub old_line: usize,
}

fn change(kind: ChangeKind, now: &Todo, before: &Todo) -> TodoChange {
    TodoChange {
        kind,
        number: now.number,
        text: now.text.clone(),
        line: now.line,
        old_line: before.line,
    }
}

/// How alike two task texts must be for one to read as the other edited.
const SIMILAR: f32 = 0.5;

/// How alike `a` and `b` are, from 0 to 1, by characters.
fn similarity(a: &str, b: &str) -> f32 {
    similar::TextDiff::from_chars(a, b).ratio()
}

/// The task changes from `old` to `new`, in the texts' order.
pub fn changes(old: &str, new: &str) -> Vec<TodoChange> {
    let before = todos(old);
    let after = todos(new);
    let old_texts: Vec<&str> = before.iter().map(|todo| todo.text.as_str()).collect();
    let new_texts: Vec<&str> = after.iter().map(|todo| todo.text.as_str()).collect();
    let mut out = Vec::new();
    for op in capture_diff_slices(Algorithm::Myers, &old_texts, &new_texts) {
        match op {
            DiffOp::Equal {
                old_index,
                new_index,
                len,
            } => {
                for k in 0..len {
                    let (was, now) = (&before[old_index + k], &after[new_index + k]);
                    if was.done != now.done {
                        let kind = if now.done {
                            ChangeKind::Checked
                        } else {
                            ChangeKind::Unchecked
                        };
                        out.push(change(kind, now, was));
                    }
                }
            }
            DiffOp::Insert {
                new_index, new_len, ..
            } => {
                for now in &after[new_index..new_index + new_len] {
                    out.push(change(ChangeKind::Added, now, now));
                }
            }
            DiffOp::Delete {
                old_index, old_len, ..
            } => {
                for was in &before[old_index..old_index + old_len] {
                    out.push(TodoChange {
                        kind: ChangeKind::Removed,
                        number: was.number,
                        text: was.text.clone(),
                        line: was.line,
                        old_line: was.line,
                    });
                }
            }
            DiffOp::Replace {
                old_index,
                old_len,
                new_index,
                new_len,
            } => {
                // A new task like a replaced one is that task edited; the
                // rest were added or removed.
                let olds = &before[old_index..old_index + old_len];
                let mut taken = vec![false; olds.len()];
                for now in &after[new_index..new_index + new_len] {
                    let best = olds
                        .iter()
                        .enumerate()
                        .filter(|(k, _)| !taken[*k])
                        .map(|(k, was)| (k, similarity(&was.text, &now.text)))
                        .filter(|(_, score)| *score >= SIMILAR)
                        .max_by(|a, b| a.1.total_cmp(&b.1));
                    match best {
                        Some((k, _)) => {
                            taken[k] = true;
                            out.push(change(ChangeKind::Edited, now, &olds[k]));
                        }
                        None => out.push(change(ChangeKind::Added, now, now)),
                    }
                }
                for (was, _) in olds.iter().zip(&taken).filter(|(_, taken)| !**taken) {
                    out.push(TodoChange {
                        kind: ChangeKind::Removed,
                        number: was.number,
                        text: was.text.clone(),
                        line: was.line,
                        old_line: was.line,
                    });
                }
            }
        }
    }
    out
}
