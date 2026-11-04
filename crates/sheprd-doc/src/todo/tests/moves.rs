use super::*;

#[test]
fn moves_items_with_their_children() {
    let text = "- a\n  - a1\n- b\n- c\n";
    let (edits, line) = move_item(text, 2, true).unwrap();
    assert_eq!(Edit::apply_all(text, &edits), "- b\n- a\n  - a1\n- c\n");
    assert_eq!(line, 0);
    // The shorter block is the one that moves: "- b" is deleted and put
    // back, "- a" with its child stays where it is.
    assert!(
        edits.iter().all(|edit| edit.text.len() <= "- b\n".len()),
        "{edits:?}"
    );
    let (edits, line) = move_item(text, 0, false).unwrap();
    assert_eq!(Edit::apply_all(text, &edits), "- b\n- a\n  - a1\n- c\n");
    assert_eq!(line, 1);
    let (edits, _) = move_item("- long item\n- x\n", 1, true).unwrap();
    assert_eq!(
        Edit::apply_all("- long item\n- x\n", &edits),
        "- x\n- long item\n"
    );
    assert_eq!(move_item(text, 0, true), None);
    assert_eq!(move_item(text, 3, false), None);
    // A sub-item moves only among its siblings.
    assert_eq!(move_item(text, 1, true), None);
}

#[test]
fn moves_plain_lines_and_items_with_children() {
    let text = "one\ntwo\nthree\n";
    let (edits, line) = move_item(text, 1, true).unwrap();
    assert_eq!(Edit::apply_all(text, &edits), "two\none\nthree\n");
    assert_eq!(line, 0);
    let (edits, line) = move_item(text, 1, false).unwrap();
    assert_eq!(Edit::apply_all(text, &edits), "one\nthree\ntwo\n");
    assert_eq!(line, 2);

    let text = "plain\n- parent\n  - child\n";
    let (edits, line) = move_item(text, 0, false).unwrap();
    assert_eq!(
        Edit::apply_all(text, &edits),
        "- parent\n  - child\nplain\n"
    );
    assert_eq!(line, 2);
    let moved = Edit::apply_all(text, &edits);
    let (edits, line) = move_item(&moved, 2, true).unwrap();
    assert_eq!(
        Edit::apply_all(&moved, &edits),
        "plain\n- parent\n  - child\n"
    );
    assert_eq!(line, 0);

    let text = "- item\nparagraph\n";
    let (edits, line) = move_item(text, 0, false).unwrap();
    assert_eq!(Edit::apply_all(text, &edits), "paragraph\n- item\n");
    assert_eq!(line, 1);
}

#[test]
fn does_not_move_an_indented_line_above_a_shallower_paragraph() {
    assert_eq!(move_item("para\n  indented\n", 1, true), None);
}

#[test]
fn moves_an_indented_line_down_across_a_heading() {
    let text = "## A\n  x\n## B\n";
    let (edits, line) = move_item(text, 1, false).unwrap();
    assert_eq!(Edit::apply_all(text, &edits), "## A\n## B\n  x\n");
    assert_eq!(line, 2);
    assert_eq!(move_item("para\n  indented\n", 0, false), None);
}

#[test]
fn moves_items_across_section_headings() {
    let text = "# Intro\nintro\n\n## Features\n- parent\n  - child\n";
    let (edits, line) = move_item(text, 4, true).unwrap();
    assert_eq!(
        Edit::apply_all(text, &edits),
        "# Intro\nintro\n- parent\n  - child\n\n## Features\n"
    );
    assert_eq!(line, 2);

    let text = "# Intro\n- parent\n  - child\n\n## Features\nnext\n";
    let (edits, line) = move_item(text, 1, false).unwrap();
    assert_eq!(
        Edit::apply_all(text, &edits),
        "# Intro\n\n## Features\n- parent\n  - child\nnext\n"
    );
    assert_eq!(line, 3);
}

#[test]
fn swaps_across_blank_gaps() {
    let text = "- a\n\n- b\n";
    let (edits, line) = move_item(text, 2, true).unwrap();
    assert_eq!(Edit::apply_all(text, &edits), "- b\n\n- a\n");
    assert_eq!(line, 0);
    let (edits, line) = move_item(text, 0, false).unwrap();
    assert_eq!(Edit::apply_all(text, &edits), "- b\n\n- a\n");
    assert_eq!(line, 2);
}

#[test]
fn crosses_one_heading_at_a_time() {
    let text = "## A\n### B\nx\n";
    let (edits, line) = move_item(text, 2, true).unwrap();
    let once = Edit::apply_all(text, &edits);
    assert_eq!(once, "## A\nx\n### B\n");
    assert_eq!(line, 1);
    let (edits, line) = move_item(&once, 1, true).unwrap();
    assert_eq!(Edit::apply_all(&once, &edits), "x\n## A\n### B\n");
    assert_eq!(line, 0);

    let text = "## A\n## B\nx\n";
    let (edits, line) = move_item(text, 2, true).unwrap();
    assert_eq!(Edit::apply_all(text, &edits), "## A\nx\n## B\n");
    assert_eq!(line, 1);
}

#[test]
fn handles_document_edges_and_missing_neighbors() {
    let text = "## H\nx\n";
    let (edits, line) = move_item(text, 1, true).unwrap();
    assert_eq!(Edit::apply_all(text, &edits), "x\n## H\n");
    assert_eq!(line, 0);
    assert_eq!(move_item(text, 1, false), None);
    let text = "\n\n## H\nx\n";
    let (edits, line) = move_item(text, 3, true).unwrap();
    assert_eq!(Edit::apply_all(text, &edits), "x\n\n\n## H\n");
    assert_eq!(line, 0);
    assert_eq!(move_item("preamble\n## H\nx\n", 0, true), None);
}

#[test]
fn preserves_text_without_a_final_line_break() {
    let text = "## H\nx";
    let (edits, line) = move_item(text, 1, true).unwrap();
    assert_eq!(Edit::apply_all(text, &edits), "x\n## H");
    assert_eq!(line, 0);

    let text = "x\n## H";
    let (edits, line) = move_item(text, 0, false).unwrap();
    assert_eq!(Edit::apply_all(text, &edits), "## H\nx");
    assert_eq!(line, 1);
}

#[test]
fn nested_items_do_not_cross_headings() {
    let text = "## H\n- parent\n  - child\n\n## N\ntext\n";
    assert_eq!(move_item(text, 2, true), None);
    assert_eq!(move_item(text, 2, false), None);
}

#[test]
fn headings_blanks_and_code_do_not_move() {
    assert_eq!(move_item("# H\ntext\n", 0, false), None);
    assert_eq!(move_item("text\n\nnext\n", 1, true), None);
    let code = "```\ncode\n```\ntext\n";
    assert_eq!(move_item(code, 0, false), None);
    assert_eq!(move_item(code, 1, false), None);
    assert_eq!(move_item(code, 2, false), None);
}
