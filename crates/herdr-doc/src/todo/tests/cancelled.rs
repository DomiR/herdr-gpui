use super::*;

fn apply(text: &str, edit: Edit) -> String {
    Edit::apply_all(text, &[edit])
}

#[test]
fn sets_states_and_is_idempotent() {
    let text = "- [ ] open\n- [x] done\n- [-] cancelled\n";
    assert_eq!(
        apply(text, set_state(text, 0, TaskState::Done).unwrap()),
        "- [x] open\n- [x] done\n- [-] cancelled\n"
    );
    assert_eq!(
        apply(text, set_state(text, 1, TaskState::Open).unwrap()),
        "- [ ] open\n- [ ] done\n- [-] cancelled\n"
    );
    assert_eq!(
        apply(text, set_state(text, 2, TaskState::Open).unwrap()),
        "- [ ] open\n- [x] done\n- [ ] cancelled\n"
    );
    assert_eq!(set_state(text, 0, TaskState::Open), None);
    assert_eq!(set_state(text, 1, TaskState::Done), None);
    assert_eq!(set_state(text, 2, TaskState::Cancelled), None);
}

#[test]
fn toggles_cancelled_tasks_as_specified() {
    let text = "- [ ] open\n- [x] done\n- [-] cancelled\n";
    assert_eq!(
        apply(text, toggle(text, 0).unwrap()),
        "- [x] open\n- [x] done\n- [-] cancelled\n"
    );
    assert_eq!(
        apply(text, toggle(text, 1).unwrap()),
        "- [ ] open\n- [ ] done\n- [-] cancelled\n"
    );
    assert_eq!(
        apply(text, toggle(text, 2).unwrap()),
        "- [ ] open\n- [x] done\n- [ ] cancelled\n"
    );
    assert_eq!(
        apply(text, toggle_cancel(text, 0).unwrap()),
        "- [-] open\n- [x] done\n- [-] cancelled\n"
    );
    assert_eq!(
        apply(text, toggle_cancel(text, 2).unwrap()),
        "- [ ] open\n- [x] done\n- [ ] cancelled\n"
    );
}

#[test]
fn cancelled_tasks_are_excluded_from_counts_and_resolved_lines() {
    let text = "# A\n- [ ] open\n- [x] done\n- [-] cancelled\n  - [ ] child\n";
    let lines = markdown::lines(text);
    assert_eq!(section_counts(&lines), vec![(0, 1, 3)]);
    let resolved = resolved_lines(&lines);
    assert!(resolved[2]);
    assert!(resolved[3]);
    assert!(!resolved[1]);
}

#[test]
fn enter_after_cancelled_starts_an_open_task() {
    let text = "- [-] cancelled";
    let (edit, _) = enter(text, text.len()).unwrap();
    assert_eq!(apply(text, edit), "- [-] cancelled\n- [ ] ");
}

#[test]
fn changes_name_all_state_transitions() {
    let cases = [
        ("- [ ] task\n", "- [x] task\n", ChangeKind::Checked),
        ("- [ ] task\n", "- [-] task\n", ChangeKind::Cancelled),
        ("- [x] task\n", "- [ ] task\n", ChangeKind::Unchecked),
        ("- [-] task\n", "- [ ] task\n", ChangeKind::Uncancelled),
    ];
    for (old, new, kind) in cases {
        assert_eq!(changes(old, new)[0].kind, kind);
    }
}

#[test]
fn unchanged_done_and_cancelled_tasks_have_no_changes() {
    let text = "- [x] a\n- [-] b\n";
    assert!(changes(text, text).is_empty());
}

#[test]
fn adding_a_task_does_not_report_unchanged_resolved_tasks() {
    let old = "- [x] done\n- [-] cancelled\n";
    let new = "- [x] done\n- [-] cancelled\n- [ ] unrelated\n";
    let changes = changes(old, new);
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].kind, ChangeKind::Added);
    assert_eq!(changes[0].text, "unrelated");
}
