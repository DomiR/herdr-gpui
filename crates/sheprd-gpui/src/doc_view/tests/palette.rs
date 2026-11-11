use super::*;

const DOC: &str = "# A\nintro\n## B\n- [ ] x\n  - [ ] y\n# C\nend\n";

fn run(doc: &mut Fixture<'_>, command: Command) {
    doc.cx.update(|window, cx| {
        doc.editor
            .update(cx, |editor, cx| editor.run(command, window, cx));
    });
    doc.cx.run_until_parked();
}

#[gpui::test]
fn outline_lists_labels_and_parent_folds(cx: &mut TestAppContext) {
    let doc = open(DOC, cx);
    let outline = doc
        .cx
        .update(|_, cx| doc.editor.update(cx, |editor, cx| editor.outline(cx)));

    assert_eq!(
        outline,
        vec![
            Outline {
                line: 0,
                label: "A".into(),
                depth: 1,
                parent: None,
                heading: true,
            },
            Outline {
                line: 2,
                label: "B".into(),
                depth: 2,
                parent: Some(0),
                heading: true,
            },
            Outline {
                line: 3,
                label: "x".into(),
                depth: 3,
                parent: Some(1),
                heading: false,
            },
            Outline {
                line: 5,
                label: "C".into(),
                depth: 1,
                parent: None,
                heading: true,
            },
        ]
    );
}

#[gpui::test]
fn fold_all_and_unfold_all_commands_change_visible_rows(cx: &mut TestAppContext) {
    let mut doc = open(DOC, cx);
    run(&mut doc, Command::FoldAll);
    assert_eq!(doc.rows(), vec![0, 5, 7]);
    run(&mut doc, Command::UnfoldAll);
    assert_eq!(doc.rows(), (0..8).collect::<Vec<_>>());
}

#[gpui::test]
fn fold_to_level_folds_deeper_sections(cx: &mut TestAppContext) {
    let mut doc = open(DOC, cx);
    run(&mut doc, Command::FoldToLevel(1));
    assert_eq!(doc.rows(), vec![0, 1, 2, 5, 6, 7]);
}

#[gpui::test]
fn go_to_line_unfolds_and_reveals_a_hidden_line(cx: &mut TestAppContext) {
    let mut doc = open(DOC, cx);
    run(&mut doc, Command::FoldToLevel(1));
    run(&mut doc, Command::GoToLine(4));

    assert_eq!(doc.rows(), (0..8).collect::<Vec<_>>());
    assert_eq!(
        doc.selection(),
        Selection::caret("# A\nintro\n## B\n- [ ] x\n".len())
    );
}

#[gpui::test]
fn toggle_task_checks_the_task_at_the_caret(cx: &mut TestAppContext) {
    let mut doc = open("- [ ] x\n", cx);
    doc.caret_at("x", 0);
    run(&mut doc, Command::ToggleTask);
    assert_eq!(doc.text(), "- [x] x\n");
}

#[gpui::test]
fn outline_is_capped_at_two_thousand_entries(cx: &mut TestAppContext) {
    let text: String = (0..2001)
        .map(|index| format!("# Heading {index}\nbody\n"))
        .collect();
    let doc = open(&text, cx);
    let outline = doc
        .cx
        .update(|_, cx| doc.editor.update(cx, |editor, cx| editor.outline(cx)));

    assert_eq!(outline.len(), 2000);
    assert_eq!(outline.last().map(|entry| entry.line), Some(3998));
}
