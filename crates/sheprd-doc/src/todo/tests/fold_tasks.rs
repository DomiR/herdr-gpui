use super::*;

#[test]
fn task_after_inserts_after_the_whole_block() {
    let text = "- [ ] a\n  - [x] a1\n  - [ ] a2\n- [ ] b\n";
    let (edit, caret) = task_after(text, 0, 3).unwrap();
    let expected = "- [ ] a\n  - [x] a1\n  - [ ] a2\n- [ ] \n- [ ] b\n";
    assert_eq!(apply(text, edit), expected);
    assert_eq!(caret, "- [ ] a\n  - [x] a1\n  - [ ] a2\n- [ ] ".len());
}

#[test]
fn task_after_keeps_nested_indent() {
    let text = "- [ ] a\n  - [ ] b\n    - [ ] c\n- [ ] d\n";
    let (edit, caret) = task_after(text, 1, 3).unwrap();
    let expected = "- [ ] a\n  - [ ] b\n    - [ ] c\n  - [ ] \n- [ ] d\n";
    assert_eq!(apply(text, edit), expected);
    assert_eq!(caret, "- [ ] a\n  - [ ] b\n    - [ ] c\n  - [ ] ".len());
}

#[test]
fn task_after_keeps_ordered_and_star_markers() {
    let ordered = "1. [ ] a\n   - [ ] x\n2. [ ] b\n";
    let (edit, caret) = task_after(ordered, 0, 2).unwrap();
    let expected = "1. [ ] a\n   - [ ] x\n2. [ ] \n2. [ ] b\n";
    assert_eq!(apply(ordered, edit), expected);
    assert_eq!(caret, "1. [ ] a\n   - [ ] x\n2. [ ] ".len());

    let starred = "* [ ] a\n  * [ ] child\n* [ ] b\n";
    let (edit, caret) = task_after(starred, 0, 2).unwrap();
    let expected = "* [ ] a\n  * [ ] child\n* [ ] \n* [ ] b\n";
    assert_eq!(apply(starred, edit), expected);
    assert_eq!(caret, "* [ ] a\n  * [ ] child\n* [ ] ".len());
}

#[test]
fn task_after_rejects_a_non_item() {
    assert_eq!(task_after("paragraph\n- [ ] task\n", 0, 1), None);
}
