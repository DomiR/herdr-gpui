use super::*;
use crate::{config::DocumentTheme, doc_view::theme};
use gpui::{Modifiers, Pixels, Point, point};

impl Fixture<'_> {
    fn set_theme(&mut self, choice: DocumentTheme) {
        let (theme, inks) = theme::resolve(choice, &Theme::default());
        self.cx.update(|window, cx| {
            self.editor.update(cx, |editor, cx| {
                let look = Look {
                    theme,
                    inks,
                    ..editor.look.clone()
                };
                editor.set_look(look, cx);
            });
            window.draw(cx).clear(cx);
        });
    }

    fn marker_position(&mut self, line: usize) -> Point<Pixels> {
        self.cx.update(|window, cx| {
            window.draw(cx).clear(cx);
            let editor = self.editor.read(cx);
            let source = &editor.prepared.lines[line];
            let marker = source.checkbox.as_ref().expect("task marker");
            let layouts = editor.layouts.borrow();
            let (layout, map) = layouts.get(&line).expect("visible task layout");
            let start = layout
                .position_for_index(map.to_display(marker.start - source.range.start))
                .expect("marker start");
            let end = layout
                .position_for_index(map.to_display(marker.end - source.range.start))
                .expect("marker end");
            assert_eq!(start.y, end.y);
            point((start.x + end.x) / 2., start.y + layout.line_height() / 2.)
        })
    }
}

#[gpui::test]
fn switching_theme_preserves_the_open_document_and_selection(cx: &mut TestAppContext) {
    let mut doc = open("# Plan\n- [ ] ship\n", cx);
    doc.caret_at("ship", 4);
    doc.type_text(" soon");
    doc.keys("shift-left shift-left");
    let original = doc.cx.update(|_, cx| doc.editor.read(cx).look.clone());
    let text = doc.text();
    let selection = doc.selection();
    let state = doc.cx.update(|_, cx| {
        let editor = doc.editor.read(cx);
        let buffer = editor.buffer.read(cx);
        let document = buffer.document().expect("loaded document");
        (document.version(), document.dirty())
    });
    assert!(state.1, "exercise a dirty buffer");
    for choice in [DocumentTheme::PlainTasks, DocumentTheme::App] {
        doc.set_theme(choice);
        assert_eq!(doc.text(), text);
        assert_eq!(doc.selection(), selection);
        doc.cx.update(|_, cx| {
            let editor = doc.editor.read(cx);
            let buffer = editor.buffer.read(cx);
            let document = buffer.document().expect("loaded document");
            assert_eq!((document.version(), document.dirty()), state);
            let (theme, inks) = theme::resolve(choice, &original.theme);
            assert_eq!(editor.look.theme, theme);
            assert_eq!(editor.look.inks, inks);
        });
    }
    doc.cx.update(|_, cx| {
        assert_eq!(doc.editor.read(cx).look, original);
    });
}

#[gpui::test]
fn notebook_marker_click_toggles_but_shift_click_does_not(cx: &mut TestAppContext) {
    for active in [false, true] {
        let mut doc = open("note\n- [ ] ship é🎉\n- [ ] neighbor\n", cx);
        if active {
            doc.caret_at("ship", 0);
        }
        doc.set_theme(DocumentTheme::PlainTasks);
        let position = doc.marker_position(1);
        let version = doc.cx.update(|_, cx| doc.editor.read(cx).version(cx));
        doc.cx.simulate_click(position, Modifiers::default());
        assert_eq!(doc.text(), "note\n- [x] ship é🎉\n- [ ] neighbor\n");
        let checked_version = doc.cx.update(|_, cx| doc.editor.read(cx).version(cx));
        assert_ne!(checked_version, version);
        doc.cx.simulate_click(
            position,
            Modifiers {
                shift: true,
                ..Modifiers::default()
            },
        );
        assert_eq!(doc.text(), "note\n- [x] ship é🎉\n- [ ] neighbor\n");
        assert_eq!(
            doc.cx.update(|_, cx| doc.editor.read(cx).version(cx)),
            checked_version
        );
        // Shift-click may expose the source marker; use its current layout.
        let position = doc.marker_position(1);
        doc.cx.simulate_click(position, Modifiers::default());
        assert_eq!(doc.text(), "note\n- [ ] ship é🎉\n- [ ] neighbor\n");
    }
}

#[gpui::test]
fn switching_theme_keeps_an_open_comment_draft_and_its_focus(cx: &mut TestAppContext) {
    let mut doc = open("- [ ] ship\n", cx);
    doc.caret_at("ship", 0);
    doc.keys(&primary("shift-m"));
    doc.type_text("keep this draft");
    let input = doc
        .cx
        .update(|_, cx| doc.editor.read(cx).draft.as_ref().unwrap().input.clone());
    for choice in [DocumentTheme::PlainTasks, DocumentTheme::App] {
        doc.set_theme(choice);
        doc.cx.update(|window, cx| {
            let editor = doc.editor.read(cx);
            assert_eq!(editor.draft.as_ref().unwrap().input, input);
            assert_eq!(input.read(cx).text(), "keep this draft");
            assert!(input.read(cx).focus.is_focused(window));
        });
    }
}
