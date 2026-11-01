//! Document tabs, headless: keys, todo interactions, folds, and the file.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::{DocEditor, DocFile, Docs, Look, editor, editor::Selection, is_markdown};
use crate::config::{Config, Theme};
use gpui::{Entity, TestAppContext, VisualTestContext};
use std::{path::PathBuf, time::Duration};

mod disk_sync;
mod folding;
mod saved_tab;
mod todo_keys;
mod typing;

/// A document editor over a temporary file holding `text`.
pub(super) struct Fixture<'a> {
    pub(super) editor: Entity<DocEditor>,
    pub(super) cx: &'a mut VisualTestContext,
    pub(super) path: PathBuf,
    _dir: tempfile::TempDir,
}

pub(super) fn open<'a>(text: &str, cx: &'a mut TestAppContext) -> Fixture<'a> {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("todo.md");
    std::fs::write(&path, text).unwrap();
    let opened = path.clone();
    let (editor, cx) = cx.add_window_view(|_, cx| {
        let buffer = Docs::open(&opened, cx);
        let look = Look {
            theme: Theme::default(),
            ui: Config::default().ui,
            code: Config::default().terminal,
        };
        DocEditor::new(
            crate::browser::TabId::from(1),
            buffer,
            look,
            Vec::new(),
            None,
            cx,
        )
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        let focus = editor.read(cx).focus.clone();
        window.focus(&focus, cx);
        window.draw(cx).clear(cx);
    });
    Fixture {
        editor,
        cx,
        path,
        _dir: dir,
    }
}

impl Fixture<'_> {
    pub(super) fn text(&mut self) -> String {
        self.cx
            .update(|_, cx| editor::editor_text(&self.editor, cx))
    }

    pub(super) fn selection(&mut self) -> Selection {
        self.cx.update(|_, cx| self.editor.read(cx).selection)
    }

    /// Puts the caret at the first `needle`, plus `offset`.
    pub(super) fn caret_at(&mut self, needle: &str, offset: usize) {
        let at = self.text().find(needle).expect("needle") + offset;
        self.cx.update(|_, cx| {
            self.editor
                .update(cx, |editor, cx| editor.select(Selection::caret(at), cx));
        });
    }

    pub(super) fn keys(&mut self, keys: &str) {
        self.cx.simulate_keystrokes(keys);
        self.cx.run_until_parked();
    }

    pub(super) fn type_text(&mut self, text: &str) {
        self.cx.simulate_input(text);
        self.cx.run_until_parked();
    }

    /// Lets time pass for the file's polls and writes.
    pub(super) fn wait(&mut self, millis: u64) {
        self.cx
            .background_executor
            .advance_clock(Duration::from_millis(millis));
        self.cx.run_until_parked();
    }

    pub(super) fn disk(&self) -> String {
        std::fs::read_to_string(&self.path).unwrap()
    }

    pub(super) fn rows(&mut self) -> Vec<usize> {
        self.cx.update(|_, cx| {
            self.editor.update(cx, |editor, cx| {
                editor.prepare(cx);
                editor.prepared.rows.clone()
            })
        })
    }
}

/// The primary modifier as `simulate_keystrokes` spells it.
pub(super) fn primary(key: &str) -> String {
    if crate::controls::CTRL_SHORTCUTS {
        format!("ctrl-{key}")
    } else {
        format!("cmd-{key}")
    }
}
