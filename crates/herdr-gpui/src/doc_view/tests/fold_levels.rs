use super::*;
use gpui::KeyBinding;

const DOC: &str =
    "# One\nintro\n## Two\n- parent\n  - child\n    - grandchild\n## Three\ntext\n# Four\nend";

fn bind(cx: &mut TestAppContext, fold_keys_last: bool) {
    cx.update(|cx| {
        cx.on_action(|_: &crate::actions::RunCommand, _| {});
        let app_bindings = [
            KeyBinding::new(
                &primary("1"),
                crate::actions::RunCommand {
                    command: herdr_commands::Command::TabNumber(1),
                },
                None,
            ),
            KeyBinding::new(
                &primary("0"),
                crate::actions::RunCommand {
                    command: herdr_commands::Command::ResetFontSize,
                },
                None,
            ),
        ];
        if fold_keys_last {
            cx.bind_keys(app_bindings);
            cx.bind_keys(super::super::fold_key_bindings());
        } else {
            cx.bind_keys(super::super::fold_key_bindings());
            cx.bind_keys(app_bindings);
        }
    });
}

fn open_bound<'a>(text: &str, cx: &'a mut TestAppContext) -> Fixture<'a> {
    bind(cx, true);
    open(text, cx)
}

#[gpui::test]
fn primary_zero_leaves_only_h1_lines_visible(cx: &mut TestAppContext) {
    let mut doc = open_bound(DOC, cx);
    doc.keys(&primary("0"));
    assert_eq!(doc.rows(), vec![0, 8]);
}

#[gpui::test]
fn primary_one_leaves_h1_sections_open_and_h2_sections_folded(cx: &mut TestAppContext) {
    let mut doc = open_bound(DOC, cx);
    doc.keys(&primary("1"));
    assert_eq!(doc.rows(), vec![0, 1, 2, 6, 8, 9]);
}

#[gpui::test]
fn primary_two_keeps_items_visible_and_folds_their_sub_items(cx: &mut TestAppContext) {
    let mut doc = open_bound(DOC, cx);
    doc.keys(&primary("2"));
    assert_eq!(doc.rows(), vec![0, 1, 2, 3, 6, 7, 8, 9]);
}

#[gpui::test]
fn opening_a_folded_h2_keeps_its_children_folded(cx: &mut TestAppContext) {
    let mut doc = open_bound(DOC, cx);
    doc.keys(&primary("1"));
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
    let mut folded = open_bound(text, cx);
    folded.caret_at("first", 0);
    folded.keys(&primary("alt-["));
    folded.keys(&primary("1"));
    assert_eq!(folded.rows(), vec![0, 1, 2, 6, 8]);

    let mut open_code = open_bound(text, cx);
    open_code.keys(&primary("1"));
    assert_eq!(open_code.rows(), vec![0, 1, 2, 3, 4, 5, 6, 8]);
}

#[gpui::test]
fn a_caret_inside_newly_hidden_lines_moves_to_the_folding_line(cx: &mut TestAppContext) {
    let mut doc = open_bound("# One\nintro\n## Two\nbody\n# Three\nend", cx);
    doc.caret_at("body", 0);
    doc.keys(&primary("1"));
    let h2_end = doc.text().find("## Two").unwrap() + "## Two".len();
    assert_eq!(doc.selection(), Selection::caret(h2_end));
}

#[gpui::test]
fn unfold_all_after_primary_zero_shows_every_line(cx: &mut TestAppContext) {
    let mut doc = open_bound("# One\ntext\n## Two\nbody\n# Three\nend", cx);
    doc.keys(&primary("0"));
    doc.keys(&primary("alt-shift-]"));
    assert_eq!(doc.rows(), vec![0, 1, 2, 3, 4, 5]);
}

#[gpui::test]
fn primary_digit_folds_in_a_document_despite_the_tab_shortcut(cx: &mut TestAppContext) {
    let mut doc = open_bound(DOC, cx);
    doc.keys(&primary("1"));
    assert_eq!(doc.rows(), vec![0, 1, 2, 6, 8, 9]);
}

#[gpui::test]
fn alt_digits_fold_like_the_plain_digits(cx: &mut TestAppContext) {
    let mut doc = open_bound(DOC, cx);
    doc.keys(&primary("alt-1"));
    assert_eq!(doc.rows(), vec![0, 1, 2, 6, 8, 9]);
    doc.keys(&primary("alt-0"));
    assert_eq!(doc.rows(), vec![0, 8]);
}

/// Proves the precedence test can fail, and why bind_keys binds the fold keys
/// after the keymap.
#[gpui::test]
fn the_tab_shortcut_wins_when_bound_after_the_fold_keys(cx: &mut TestAppContext) {
    bind(cx, false);
    let mut doc = open(DOC, cx);
    doc.keys(&primary("1"));
    assert_eq!(doc.rows(), (0..=9).collect::<Vec<_>>());
}
