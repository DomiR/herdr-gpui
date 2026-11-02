use super::*;

const DOC: &str =
    "# One\nintro\n## Two\n- parent\n  - child\n    - grandchild\n## Three\ntext\n# Four\nend";

#[gpui::test]
fn alt_zero_leaves_only_h1_lines_visible(cx: &mut TestAppContext) {
    let mut doc = open(DOC, cx);
    doc.keys(&primary("alt-0"));
    assert_eq!(doc.rows(), vec![0, 8]);
}

#[gpui::test]
fn alt_one_leaves_h1_sections_open_and_h2_sections_folded(cx: &mut TestAppContext) {
    let mut doc = open(DOC, cx);
    doc.keys(&primary("alt-1"));
    assert_eq!(doc.rows(), vec![0, 1, 2, 6, 8, 9]);
}

#[gpui::test]
fn alt_two_keeps_items_visible_and_folds_their_sub_items(cx: &mut TestAppContext) {
    let mut doc = open(DOC, cx);
    doc.keys(&primary("alt-2"));
    assert_eq!(doc.rows(), vec![0, 1, 2, 3, 6, 7, 8, 9]);
}

#[gpui::test]
fn opening_a_folded_h2_keeps_its_children_folded(cx: &mut TestAppContext) {
    let mut doc = open(DOC, cx);
    doc.keys(&primary("alt-1"));
    doc.caret_at("## Two", 0);
    doc.keys(&primary("alt-]"));
    assert_eq!(doc.rows(), vec![0, 1, 2, 3, 6, 8, 9]);
}

#[gpui::test]
fn alt_left_bracket_still_folds_the_innermost_fold(cx: &mut TestAppContext) {
    let mut doc = open(DOC, cx);
    doc.caret_at("child", 0);
    doc.keys(&primary("alt-["));
    assert_eq!(doc.rows(), vec![0, 1, 2, 3, 4, 6, 7, 8, 9]);
}

#[gpui::test]
fn code_fold_state_survives_level_folding(cx: &mut TestAppContext) {
    let text = "# One\nintro\n```\nfirst\nsecond\n```\n## Two\ntext\n## Three\nend";
    let mut folded = open(text, cx);
    folded.caret_at("first", 0);
    folded.keys(&primary("alt-["));
    folded.keys(&primary("alt-1"));
    assert_eq!(folded.rows(), vec![0, 1, 2, 6, 8]);

    let mut open_code = open(text, cx);
    open_code.keys(&primary("alt-1"));
    assert_eq!(open_code.rows(), vec![0, 1, 2, 3, 4, 5, 6, 8]);
}

#[gpui::test]
fn a_caret_inside_newly_hidden_lines_moves_to_the_folding_line(cx: &mut TestAppContext) {
    let mut doc = open("# One\nintro\n## Two\nbody\n# Three\nend", cx);
    doc.caret_at("body", 0);
    doc.keys(&primary("alt-1"));
    let h2_end = doc.text().find("## Two").unwrap() + "## Two".len();
    assert_eq!(doc.selection(), Selection::caret(h2_end));
}

#[gpui::test]
fn unfold_all_after_alt_zero_shows_every_line(cx: &mut TestAppContext) {
    let mut doc = open("# One\ntext\n## Two\nbody\n# Three\nend", cx);
    doc.keys(&primary("alt-0"));
    doc.keys(&primary("alt-shift-]"));
    assert_eq!(doc.rows(), vec![0, 1, 2, 3, 4, 5]);
}

#[test]
fn altgr_digit_typing_is_not_a_level_shortcut() {
    let event = gpui::KeyDownEvent {
        keystroke: gpui::Keystroke {
            modifiers: gpui::Modifiers {
                control: true,
                alt: true,
                ..gpui::Modifiers::default()
            },
            key: "7".into(),
            key_char: Some("{".into()),
        },
        is_held: false,
        prefer_character_input: true,
    };
    assert_eq!(
        super::super::keys::alt_gr_typing(&event),
        crate::controls::CTRL_SHORTCUTS
    );
}
