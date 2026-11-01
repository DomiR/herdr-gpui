//! Document tabs: a Markdown file edited in place, live-styled, with task
//! lists, folds, and other writers' changes merged in as they happen
//! (`docs/markdown-docs-plan.md`). The file stays plain Markdown on disk.
//!
//! A tab keeps the file's path, the line it opened at, and the sections
//! folded in it; the window keeps each tab's editor; the app keeps one
//! buffer per file, shared by every editor and by agents' `doc` commands.

// i18n: migrated
mod buffer;
mod editor;
mod error;
mod input;
mod keys;
mod layout;
mod render;

pub(crate) use buffer::{DocBuffer, Docs, State};
pub(crate) use editor::{DocEditor, FoldsChanged};
pub use error::Error;
pub(crate) use layout::Look;

use crate::{
    HerdrWindow,
    browser::{Location, Slot, Store, Tab, TabId},
    editor::EditorTarget,
    i18n::fl,
    window::Flash,
};
use gpui::{prelude::*, *};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// The most folded sections a tab saves.
const MAX_FOLDED: usize = 256;

/// The file a document tab shows, the line it opened at, and its folded
/// sections by ID. Saved with the tabs, so it is checked again when read.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "SavedDocFile")]
pub(crate) struct DocFile {
    pub(crate) path: String,
    pub(crate) line: Option<u32>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) folded: Vec<String>,
}

#[derive(Deserialize)]
struct SavedDocFile {
    path: String,
    line: Option<u32>,
    #[serde(default)]
    folded: Vec<String>,
}

impl TryFrom<SavedDocFile> for DocFile {
    type Error = crate::Error;

    fn try_from(saved: SavedDocFile) -> crate::Result<Self> {
        if saved.path.is_empty()
            || saved.path.len() > 4096
            || saved.path.chars().any(char::is_control)
            || !Path::new(&saved.path).is_absolute()
            || !is_markdown(Path::new(&saved.path))
        {
            return Err(Error::NotMarkdown.into());
        }
        let folded = saved
            .folded
            .into_iter()
            .filter(|id| id.len() <= 256 && !id.chars().any(char::is_control))
            .take(MAX_FOLDED)
            .collect();
        Ok(Self {
            path: saved.path,
            line: saved.line,
            folded,
        })
    }
}

impl DocFile {
    /// The file's name, as its tab is titled.
    pub(crate) fn name(&self) -> &str {
        Path::new(&self.path)
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or(&self.path)
    }
}

/// Whether `path` names a Markdown file, which opens as a document.
pub(crate) fn is_markdown(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            ["md", "markdown", "mdown", "mkd"]
                .iter()
                .any(|known| extension.eq_ignore_ascii_case(known))
        })
}

impl HerdrWindow {
    /// Opens `target` in a document tab of the focused workspace, or brings
    /// back the tab already showing it, at the target's line.
    pub(crate) fn open_doc_view(
        &mut self,
        target: &EditorTarget,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<TabId> {
        let Some((scope, workspace)) = self.browser_key() else {
            self.show_flash(Flash::warning(fl!("doc-view-no-workspace")), cx);
            return None;
        };
        let Some(path) = target.path.to_str().map(str::to_owned) else {
            self.show_flash(Flash::warning(fl!("doc-view-invalid-path")), cx);
            return None;
        };
        let existing = cx.try_global::<Store>().and_then(|store| {
            store
                .in_workspace(&scope, &workspace)
                .find(|tab| matches!(&tab.location, Some(Location::Doc { file }) if file.path == path))
                .map(|tab| tab.id)
        });
        let file = DocFile {
            path,
            line: target.line,
            folded: Vec::new(),
        };
        let opened = existing.or_else(|| {
            Store::update(cx, |store| {
                store.open(scope, &workspace, Some(Location::Doc { file }), None)
            })
        });
        let Some(id) = opened else {
            self.show_flash(Flash::warning(fl!("doc-view-too-many-tabs")), cx);
            return None;
        };
        self.ensure_doc_view(id, cx);
        let editor = self.doc_views.get(&id)?.clone();
        if let Some(line) = target.line {
            editor.update(cx, |editor, cx| {
                editor.go_to_line(line.saturating_sub(1) as usize, cx);
            });
        }
        self.show_browser_tab(id, window, cx);
        let focus = editor.read(cx).focus.clone();
        window.focus(&focus, cx);
        cx.notify();
        Some(id)
    }

    /// Makes `id`'s editor from its tab, if the window has none yet.
    fn ensure_doc_view(&mut self, id: TabId, cx: &mut Context<Self>) {
        if self.doc_views.contains_key(&id) {
            return;
        }
        let Some(Location::Doc { file }) = cx
            .try_global::<Store>()
            .and_then(|store| store.get(id))
            .and_then(|tab| tab.location.clone())
        else {
            return;
        };
        let buffer = Docs::open(Path::new(&file.path), cx);
        let look = self.doc_look();
        let editor = cx.new(|cx| DocEditor::new(id, buffer, look, file.folded, file.line, cx));
        cx.subscribe(&editor, |_, editor, event: &FoldsChanged, cx| {
            let id = editor.read(cx).tab();
            save_folds(id, event.0.clone(), cx);
        })
        .detach();
        self.doc_views.insert(id, editor);
    }

    fn doc_look(&self) -> Look {
        Look {
            theme: self.theme.clone(),
            ui: self.config.ui.clone(),
            code: self.config.terminal.clone(),
        }
    }

    /// Runs every window tick: document tabs of the focused workspace with
    /// no editor yet get one; the editors of closed tabs go.
    pub(crate) fn poll_doc_views(&mut self, cx: &mut Context<Self>) {
        let Some(store) = cx.try_global::<Store>() else {
            self.doc_views.clear();
            return;
        };
        self.doc_views.retain(|id, _| store.get(*id).is_some());
        let Some((scope, workspace)) = self.browser_key() else {
            return;
        };
        let missing: Vec<TabId> = store
            .in_workspace(&scope, &workspace)
            .filter(|tab| {
                matches!(tab.location, Some(Location::Doc { .. }))
                    && !self.doc_views.contains_key(&tab.id)
            })
            .map(|tab| tab.id)
            .collect();
        for id in missing {
            self.ensure_doc_view(id, cx);
        }
    }

    /// Whether a document editor holds the keyboard; the window then leaves
    /// keys to it.
    pub(crate) fn doc_focused(&self, window: &Window, cx: &App) -> bool {
        self.doc_views
            .values()
            .any(|editor| editor.read(cx).focus.contains_focused(window, cx))
    }

    pub(crate) fn render_doc_tab(
        &mut self,
        slot: Slot,
        tab: &Tab,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(editor) = self.doc_views.get(&tab.id).cloned() else {
            return div()
                .id(SharedString::from(slot.selector("doc-loading")))
                .size_full()
                .into_any_element();
        };
        let look = self.doc_look();
        editor.update(cx, |editor, cx| editor.set_look(look, cx));
        div()
            .id(SharedString::from(slot.selector("doc-tab")))
            .size_full()
            .min_w_0()
            .child(editor)
            .into_any_element()
    }
}

/// Saves which sections tab `id` has folded, with the tab.
fn save_folds(id: TabId, folded: Vec<String>, cx: &mut App) {
    let Some(Location::Doc { file }) = cx
        .try_global::<Store>()
        .and_then(|store| store.get(id))
        .and_then(|tab| tab.location.clone())
    else {
        return;
    };
    let file = DocFile {
        folded: folded.into_iter().take(MAX_FOLDED).collect(),
        ..file
    };
    Store::update(cx, |store| {
        store.visited(id, Some(Location::Doc { file }), None)
    });
}

#[cfg(test)]
mod tests;
