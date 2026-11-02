#![allow(clippy::unwrap_used)]
use super::edit::{Editing, Format};

fn typed(text: &str) -> Editing {
    let mut edit = Editing::default();
    edit.replace(None, text, false, None);
    edit
}

fn select(edit: &mut Editing, range: std::ops::Range<usize>) {
    edit.anchor = range.start;
    edit.cursor = range.end;
}

#[test]
fn bold_wraps_the_selection_and_unwraps_it_again() {
    let mut edit = typed("make it bold");
    select(&mut edit, 8..12);
    edit.format(Format::Bold);
    assert_eq!(edit.text, "make it **bold**");
    assert_eq!(edit.selection(), 10..14);
    edit.format(Format::Bold);
    assert_eq!(edit.text, "make it bold");
    assert_eq!(edit.selection(), 8..12);
}

#[test]
fn a_bare_caret_gets_both_markers_around_it() {
    let mut edit = typed("x ");
    edit.format(Format::Code);
    assert_eq!(edit.text, "x ``");
    assert_eq!(edit.selection(), 3..3);
}

#[test]
fn a_link_selects_its_address_to_type_over() {
    let mut edit = typed("see docs");
    select(&mut edit, 4..8);
    edit.format(Format::Link);
    assert_eq!(edit.text, "see [docs](url)");
    assert_eq!(&edit.text[edit.selection()], "url");
}

#[test]
fn line_formats_toggle_on_every_selected_line() {
    let mut edit = typed("one\ntwo");
    select(&mut edit, 0..7);
    edit.format(Format::Tasks);
    assert_eq!(edit.text, "- [ ] one\n- [ ] two");
    let all = 0..edit.text.len();
    select(&mut edit, all);
    edit.format(Format::Tasks);
    assert_eq!(edit.text, "one\ntwo");
}

#[test]
fn enter_continues_a_list_and_an_empty_item_ends_it() {
    let mut edit = typed("- first");
    edit.newline();
    assert_eq!(edit.text, "- first\n- ");
    edit.newline();
    assert_eq!(edit.text, "- first\n");
    let mut tasks = typed("  - [x] done");
    tasks.newline();
    assert_eq!(tasks.text, "  - [x] done\n  - [ ] ");
}

#[test]
fn undo_and_redo_step_through_edits() {
    let mut edit = typed("a");
    edit.replace(None, "b", false, None);
    assert_eq!(edit.text, "ab");
    assert!(edit.undo());
    assert_eq!(edit.text, "a");
    assert!(edit.undo());
    assert_eq!(edit.text, "");
    assert!(edit.redo());
    assert_eq!(edit.text, "a");
}

#[test]
fn pasted_text_keeps_newlines_but_not_other_controls() {
    let edit = typed("one\r\ntwo\u{7}\tthree");
    assert_eq!(edit.text, "one\ntwo\tthree");
}

#[test]
fn line_ends_follow_newlines() {
    let edit = typed("one\ntwo\nthree");
    assert_eq!(edit.line_start(5), 4);
    assert_eq!(edit.line_end(5), 7);
    assert_eq!(edit.line_end(9), 13);
}
