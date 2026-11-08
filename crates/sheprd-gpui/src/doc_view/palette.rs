//! Commands and jump list data exposed to the document command palette.

use super::editor::DocEditor;
use gpui::{Context, Window};
use std::ops::Range;

const MAX_OUTLINE: usize = 2000;
const MAX_LABEL_CHARS: usize = 120;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Command {
    CancelTask,
    Find,
    FoldAll,
    FoldToLevel(usize),
    GoToLine(usize),
    ToggleHideDone,
    ToggleTask,
    UnfoldAll,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Outline {
    pub(crate) line: usize,
    pub(crate) label: String,
    pub(crate) depth: usize,
    pub(crate) parent: Option<usize>,
    pub(crate) heading: bool,
}

impl DocEditor {
    /// Headings and folds in document order, at most 2000.
    pub(crate) fn outline(&mut self, cx: &mut Context<Self>) -> Vec<Outline> {
        self.prepare(cx);
        let text = self.text(cx);
        let mut outlines = Vec::new();
        let mut included: Vec<(usize, Range<usize>)> = Vec::new();

        for fold in &self.prepared.folds {
            if outlines.len() >= MAX_OUTLINE {
                break;
            }
            let Some(line) = self.prepared.lines.get(fold.line) else {
                continue;
            };
            let mut label = text[line.content()].trim();
            if line.is_heading() {
                label = label.trim_end_matches('#').trim();
            }
            if label.is_empty() {
                continue;
            }
            let label: String = label.chars().take(MAX_LABEL_CHARS).collect();
            let parent = included
                .iter()
                .filter(|(_, hidden)| hidden.contains(&fold.line))
                .max_by_key(|(_, hidden)| hidden.start)
                .map(|(index, _)| *index);
            let index = outlines.len();
            outlines.push(Outline {
                line: fold.line,
                label,
                depth: fold.depth,
                parent,
                heading: line.is_heading(),
            });
            included.push((index, fold.hidden.clone()));
        }
        outlines
    }

    pub(crate) fn run(&mut self, command: Command, window: &mut Window, cx: &mut Context<Self>) {
        self.prepare(cx);
        match command {
            Command::CancelTask => self.cancel_task(cx),
            Command::Find => self.open_find(window, cx),
            Command::FoldAll => self.fold_all(true, cx),
            Command::FoldToLevel(level) => self.fold_to_level(level, cx),
            Command::GoToLine(line) => self.go_to_line(line, cx),
            Command::ToggleHideDone => self.toggle_hide_done(cx),
            Command::ToggleTask => self.cycle_task(cx),
            Command::UnfoldAll => self.fold_all(false, cx),
        }
        cx.notify();
    }
}
