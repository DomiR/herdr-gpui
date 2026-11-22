use super::*;

#[core::prelude::v1::test]
fn a_note_is_drawn_line_by_line_without_controls_and_bounded() {
    assert_eq!(
        lines("<sheprd>\n\nBe\u{202e} brief.\u{7}\n</sheprd>"),
        ["<sheprd>", " ", "Be brief.", "</sheprd>"]
    );
    let long = "x".repeat(MAX_SHOWN);
    let shown = lines(&format!("{long}\nmore"));
    assert_eq!(shown, [long, "…".to_owned()]);
}

#[core::prelude::v1::test]
fn a_note_of_blank_lines_draws_no_more_than_the_line_cap() {
    let shown = lines(&"\n".repeat(MAX_SHOWN));
    assert_eq!(shown.len(), MAX_LINES + 1);
    assert_eq!(shown.last().map(String::as_str), Some("…"));
    assert_eq!(lines(&"\n".repeat(MAX_LINES - 1)).len(), MAX_LINES - 1);
}
