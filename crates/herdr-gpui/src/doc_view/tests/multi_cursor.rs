use super::*;

impl Fixture<'_> {
    fn add_cursor(&mut self, at: usize) {
        self.cx.update(|_, cx| {
            self.editor.update(cx, |editor, cx| {
                editor.add_cursor(Selection::caret(at), cx);
            });
        });
        self.cx.run_until_parked();
    }

    fn extra_cursors(&mut self) -> Vec<Selection> {
        self.cx.update(|_, cx| self.editor.read(cx).extra_cursors())
    }

    fn bind_multi_cursor(&mut self) {
        self.cx
            .update(|_, cx| cx.bind_keys(crate::doc_view::key_bindings()));
    }

    fn select(&mut self, selection: Selection) {
        self.cx.update(|_, cx| {
            self.editor
                .update(cx, |editor, cx| editor.select(selection, cx));
        });
        self.cx.run_until_parked();
    }
}

#[gpui::test]
fn typing_at_three_cursors_is_one_undo_step(cx: &mut TestAppContext) {
    let mut doc = open("a b c\n", cx);
    doc.caret_at("a", 0);
    doc.add_cursor(2);
    doc.add_cursor(4);
    doc.type_text("X");
    assert_eq!(doc.text(), "Xa Xb Xc\n");
    assert_eq!(
        doc.extra_cursors(),
        vec![Selection::caret(4), Selection::caret(7)]
    );
    assert_eq!(doc.selection(), Selection::caret(1));
    doc.keys(&primary("z"));
    assert_eq!(doc.text(), "a b c\n");
    assert!(doc.extra_cursors().is_empty());
    doc.keys(&primary("shift-z"));
    assert_eq!(doc.text(), "Xa Xb Xc\n");
    assert!(doc.extra_cursors().is_empty());
}

#[gpui::test]
fn backspace_merges_overlapping_ranges(cx: &mut TestAppContext) {
    let mut doc = open("abc\n", cx);
    doc.caret_at("a", 0);
    doc.add_cursor(1);
    doc.add_cursor(2);
    doc.keys("backspace");
    assert_eq!(doc.text(), "c\n");
}

#[gpui::test]
fn delete_merges_overlapping_ranges(cx: &mut TestAppContext) {
    let mut doc = open("abcd\n", cx);
    doc.caret_at("a", 0);
    doc.add_cursor(1);
    doc.add_cursor(3);
    doc.keys("delete");
    assert_eq!(doc.text(), "c\n");
}

#[gpui::test]
fn extra_backspace_uses_the_list_marker_rule(cx: &mut TestAppContext) {
    let mut doc = open("- a\n- b\n", cx);
    doc.caret_at("- a", 0);
    doc.add_cursor(6);
    doc.keys("backspace");
    assert_eq!(doc.text(), "- a\nb\n");
}

#[gpui::test]
fn main_only_enter_moves_extra_cursors(cx: &mut TestAppContext) {
    let mut doc = open("ab\n", cx);
    doc.caret_at("a", 0);
    doc.add_cursor(2);
    doc.keys("enter");
    assert_eq!(doc.text(), "\nab\n");
    assert_eq!(doc.selection(), Selection::caret(1));
    assert_eq!(doc.extra_cursors(), vec![Selection::caret(3)]);
}

#[gpui::test]
fn cmd_enter_moves_extra_cursors(cx: &mut TestAppContext) {
    let mut doc = open("buy milk\nlater\n", cx);
    doc.caret_at("buy", 0);
    doc.add_cursor("buy milk\n".len());
    doc.keys(&primary("enter"));
    assert_eq!(doc.text(), "- [ ] buy milk\nlater\n");
    assert_eq!(doc.extra_cursors(), vec![Selection::caret(15)]);
}

#[gpui::test]
fn cmd_d_selects_and_replaces_all_occurrences(cx: &mut TestAppContext) {
    let mut doc = open("one two one one\n", cx);
    doc.bind_multi_cursor();
    doc.caret_at("one", 0);
    doc.keys(&primary("d"));
    assert_eq!(doc.selection(), Selection { anchor: 0, head: 3 });
    assert!(doc.extra_cursors().is_empty());
    doc.keys(&primary("d"));
    doc.keys(&primary("d"));
    assert_eq!(
        doc.extra_cursors(),
        vec![
            Selection {
                anchor: 8,
                head: 11
            },
            Selection {
                anchor: 12,
                head: 15
            },
        ]
    );
    doc.keys(&primary("d"));
    assert_eq!(
        doc.extra_cursors(),
        vec![
            Selection {
                anchor: 8,
                head: 11
            },
            Selection {
                anchor: 12,
                head: 15
            },
        ]
    );
    doc.type_text("X");
    assert_eq!(doc.text(), "X two X X\n");
    assert_eq!(
        doc.extra_cursors(),
        vec![Selection::caret(7), Selection::caret(9)]
    );
}

#[gpui::test]
fn cmd_d_adds_an_adjacent_occurrence(cx: &mut TestAppContext) {
    let mut doc = open("abab\n", cx);
    doc.bind_multi_cursor();
    doc.select(Selection { anchor: 0, head: 2 });
    doc.keys(&primary("d"));
    assert_eq!(doc.extra_cursors(), vec![Selection { anchor: 2, head: 4 }]);
}

#[gpui::test]
fn cmd_d_adjacent_occurrences_type_at_both(cx: &mut TestAppContext) {
    let mut doc = open("abab\n", cx);
    doc.bind_multi_cursor();
    doc.select(Selection { anchor: 0, head: 2 });
    doc.keys(&primary("d"));
    doc.type_text("X");
    assert_eq!(doc.text(), "XX\n");
    assert_eq!(doc.selection(), Selection::caret(1));
    assert_eq!(doc.extra_cursors(), vec![Selection::caret(2)]);
}

#[gpui::test]
fn overlapping_cursors_are_normalized(cx: &mut TestAppContext) {
    let mut doc = open("abcdef\n", cx);
    doc.select(Selection { anchor: 1, head: 4 });
    doc.add_cursor(2);
    assert!(doc.extra_cursors().is_empty());
    doc.add_cursor(5);
    doc.add_cursor(5);
    assert_eq!(doc.extra_cursors(), vec![Selection::caret(5)]);
}

#[gpui::test]
fn escape_keeps_the_main_selection_and_clears_extras(cx: &mut TestAppContext) {
    let mut doc = open("abcdef\n", cx);
    doc.select(Selection { anchor: 1, head: 4 });
    doc.add_cursor(5);
    doc.keys("escape");
    assert_eq!(doc.selection(), Selection { anchor: 1, head: 4 });
    assert!(doc.extra_cursors().is_empty());
}

#[gpui::test]
fn another_writer_moves_extra_cursors(cx: &mut TestAppContext) {
    let mut doc = open("one\n", cx);
    doc.caret_at("one", 0);
    doc.add_cursor(2);
    doc.keys("right");
    std::fs::write(&doc.path, "prefixone\n").unwrap();
    doc.wait(600);
    assert_eq!(doc.selection(), Selection::caret(7));
    assert_eq!(doc.extra_cursors(), vec![Selection::caret(8)]);
}

#[gpui::test]
fn folding_a_line_drops_an_extra_cursor_inside_it(cx: &mut TestAppContext) {
    let mut doc = open("# One\ntext\n# Two\n", cx);
    doc.caret_at("# One", 0);
    doc.add_cursor("# One\n".len());
    doc.cx.update(|_, cx| {
        doc.editor
            .update(cx, |editor, cx| editor.toggle_fold(0, cx));
    });
    doc.cx.run_until_parked();
    assert!(doc.extra_cursors().is_empty());
}

#[gpui::test]
fn cmd_d_skips_occurrences_inside_folded_sections(cx: &mut TestAppContext) {
    let mut doc = open("# A\nfoo\n# B\nfoo\nfoo\n", cx);
    doc.bind_multi_cursor();
    doc.cx.update(|_, cx| {
        doc.editor
            .update(cx, |editor, cx| editor.toggle_fold(0, cx));
    });
    doc.cx.run_until_parked();
    let visible = "# A\nfoo\n# B\n".len();
    doc.select(Selection {
        anchor: visible,
        head: visible + 3,
    });
    doc.keys(&primary("d"));
    assert_eq!(
        doc.extra_cursors(),
        vec![Selection {
            anchor: visible + 4,
            head: visible + 7,
        }]
    );
    doc.keys(&primary("d"));
    assert_eq!(
        doc.extra_cursors(),
        vec![Selection {
            anchor: visible + 4,
            head: visible + 7,
        }]
    );
}
