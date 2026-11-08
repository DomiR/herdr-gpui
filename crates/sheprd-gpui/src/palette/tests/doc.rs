use super::*;
use crate::{
    browser::TabId,
    config::{Config, DocumentTheme, Theme},
    doc_view::{DocEditor, Docs, Look},
};
use gpui::{Entity, TestAppContext, VisualTestContext};
use std::path::PathBuf;

struct Fixture<'a> {
    view: Entity<SheprdWindow>,
    editor: Entity<DocEditor>,
    cx: &'a mut VisualTestContext,
    _dir: tempfile::TempDir,
}

fn open_doc<'a>(text: &str, cx: &'a mut TestAppContext) -> Fixture<'a> {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("document.md");
    std::fs::write(&path, text).unwrap();
    let opened: PathBuf = path.clone();
    let (view, cx) = cx.add_window_view(move |window, cx| {
        let mut view = super::super::fixture_window(window, cx);
        let buffer = Docs::open(&opened, cx);
        let look = Look {
            theme: Theme::default(),
            inks: None,
            ui: Config::default().ui,
            code: Config::default().terminal,
        };
        let editor =
            cx.new(|cx| DocEditor::new(TabId::from(1), buffer, look, Vec::new(), None, cx));
        view.doc_views.insert(TabId::from(1), editor);
        view
    });
    cx.run_until_parked();
    let editor = cx
        .update(|_, cx| view.read(cx).doc_views.get(&TabId::from(1)).cloned())
        .unwrap();
    cx.update(|window, cx| window.focus(&editor.read(cx).focus, cx));
    Fixture {
        view,
        editor,
        cx,
        _dir: dir,
    }
}

#[test]
fn document_filter_accepts_only_document_rows() {
    let document = Action::Doc(DocAction::Run(crate::doc_view::Command::Find));
    let native = Action::Native(Command::Palette);
    assert!(Filter::Document.accepts(&document));
    assert!(!Filter::Document.accepts(&native));
    assert!(Filter::All.accepts(&document));
}

#[gpui::test]
fn palette_default_filter_follows_document_focus(cx: &mut TestAppContext) {
    let mut doc = open_doc("# A\ntext\n", cx);
    doc.cx.update(|window, cx| {
        assert_eq!(
            doc.view.read(cx).palette_default_filter(window, cx),
            Filter::Document
        );
        let focus = doc.view.read(cx).focus.clone();
        window.focus(&focus, cx);
        assert_eq!(
            doc.view.read(cx).palette_default_filter(window, cx),
            Filter::All
        );
    });
}

#[gpui::test]
fn captured_documents_add_commands_and_nested_outline_rows(cx: &mut TestAppContext) {
    let mut doc = open_doc("# A\nintro\n## B\n- [ ] x\n  - [ ] y\n# C\nend\n", cx);
    doc.cx.update(|window, cx| {
        doc.view.update(cx, |view, cx| {
            view.open_palette(Filter::Document, window, cx)
        });
    });
    let rows = doc.cx.update(|_, cx| {
        let palette = doc.view.read(cx).menu.palette.as_ref().unwrap();
        palette
            .entries
            .iter()
            .enumerate()
            .filter_map(|(index, entry)| match &entry.action {
                Action::Doc(DocAction::Run(crate::doc_view::Command::GoToLine(line))) => {
                    Some((index, entry.label.to_string(), *line, entry.parent))
                }
                Action::Doc(_) => None,
                _ => None,
            })
            .collect::<Vec<_>>()
    });
    assert_eq!(rows.len(), 4);
    assert_eq!(rows[0].1, "A");
    assert_eq!(rows[1].1, "B");
    assert_eq!(rows[2].1, "x");
    assert_eq!(rows[3].1, "C");
    assert_eq!(rows[0].3, None);
    assert_eq!(rows[1].3, Some(rows[0].0));
    assert_eq!(rows[2].3, Some(rows[1].0));
    assert_eq!(rows[3].3, None);
}

#[gpui::test]
fn no_document_has_no_document_rows_or_filter_chip(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.open_palette(Filter::All, window, cx));
    });
    view.read_with(cx, |view, _| {
        let palette = view.menu.palette.as_ref().unwrap();
        assert!(palette.doc.is_none());
        assert!(
            palette
                .entries
                .iter()
                .all(|entry| !matches!(entry.action, Action::Doc(_)))
        );
    });
}

#[gpui::test]
fn current_theme_is_marked_and_other_theme_updates_config(cx: &mut TestAppContext) {
    let mut doc = open_doc("# A\ntext\n", cx);
    doc.cx.update(|window, cx| {
        doc.view.update(cx, |view, cx| {
            view.open_palette(Filter::Document, window, cx)
        });
    });
    let (plain, app) = doc.cx.update(|_, cx| {
        let palette = doc.view.read(cx).menu.palette.as_ref().unwrap();
        let mut themes = palette
            .entries
            .iter()
            .filter_map(|entry| match &entry.action {
                Action::Doc(DocAction::Theme(DocumentTheme::PlainTasks)) => {
                    Some((entry.detail.to_string(), entry.action.clone()))
                }
                Action::Doc(DocAction::Theme(DocumentTheme::App)) => {
                    Some((entry.detail.to_string(), entry.action.clone()))
                }
                _ => None,
            });
        (themes.next().unwrap(), themes.next().unwrap())
    });
    assert_eq!(plain.0, "Current");
    assert_eq!(app.0, "");
    doc.cx.update(|window, cx| {
        doc.view
            .update(cx, |view, cx| view.activate_palette(app.1, window, cx));
    });
    assert_eq!(
        doc.view
            .read_with(doc.cx, |view, _| view.config.documents.theme),
        DocumentTheme::App
    );
}

#[gpui::test]
fn activating_an_outline_row_closes_palette_and_focuses_the_editor(cx: &mut TestAppContext) {
    let mut doc = open_doc("# A\nintro\n## B\nbody\n# C\nend\n", cx);
    doc.cx.update(|window, cx| {
        doc.view.update(cx, |view, cx| {
            view.open_palette(Filter::Document, window, cx)
        });
    });
    let action = doc.cx.update(|_, cx| {
        let palette = doc.view.read(cx).menu.palette.as_ref().unwrap();
        palette
            .entries
            .iter()
            .find_map(|entry| match &entry.action {
                Action::Doc(DocAction::Run(crate::doc_view::Command::GoToLine(line)))
                    if line == 3 =>
                {
                    Some(entry.action.clone())
                }
                _ => None,
            })
            .unwrap()
    });
    doc.cx.update(|window, cx| {
        doc.view
            .update(cx, |view, cx| view.activate_palette(action, window, cx));
        assert!(doc.editor.read(cx).focus.is_focused(window));
    });
    assert!(
        doc.cx
            .update(|_, cx| doc.view.read(cx).menu.palette.is_none())
    );
    assert_eq!(
        doc.cx.update(|_, cx| doc.editor.read(cx).caret()),
        "# A\nintro\n## B\n".len()
    );
}
