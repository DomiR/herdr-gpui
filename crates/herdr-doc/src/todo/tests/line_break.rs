#![allow(clippy::unwrap_used)]

use super::*;

fn apply(text: &str, edit: Edit) -> String {
    Edit::apply_all(text, &[edit])
}

#[test]
fn indents_under_task_text() {
    let text = "- [ ] text";
    let (edit, caret) = line_break(text, text.len());
    assert_eq!(apply(text, edit), "- [ ] text\n      ");
    assert_eq!(caret, "- [ ] text\n      ".len());

    let text = "- [ ] ab";
    let (edit, _) = line_break(text, 6);
    assert_eq!(apply(text, edit), "- [ ] a\n      b");

    let text = "  1. x";
    let (edit, _) = line_break(text, text.len());
    assert_eq!(apply(text, edit), "  1. x\n     ");

    let text = "-\tx";
    let (edit, _) = line_break(text, text.len());
    assert_eq!(apply(text, edit), "-\tx\n \t");
}

#[test]
fn preserves_indentation_elsewhere() {
    for (text, expected) in [
        ("      note", "      note\n      "),
        ("plain", "plain\n"),
        ("  - [ ] ", "  - [ ] \n  "),
    ] {
        let (edit, _) = line_break(text, text.len());
        assert_eq!(apply(text, edit), expected);
    }

    let text = "- [ ] x";
    let (edit, _) = line_break(text, 1);
    assert_eq!(apply(text, edit), "-\n [ ] x");
}

#[test]
fn keeps_a_note_in_the_task_block() {
    let text = "- [ ] text";
    let (edit, caret) = line_break(text, text.len());
    let split = apply(text, edit);
    let with_note = format!("{split}note");
    let lines = crate::markdown::lines(&with_note);
    assert!(lines[1].depth > lines[0].depth);
    assert!(block_end(&lines, 0) >= 2);
    assert_eq!(caret, split.len());
}
