use super::*;
use chrono::{NaiveDate, NaiveDateTime};

fn time(day: u32, hour: u32, minute: u32) -> NaiveDateTime {
    NaiveDate::from_ymd_opt(2026, 11, day)
        .unwrap()
        .and_hms_opt(hour, minute, 0)
        .unwrap()
}

fn apply_edits(text: &str, edits: Vec<Edit>) -> String {
    Edit::apply_all(text, &edits)
}

#[test]
fn check_writes_a_done_stamp() {
    let now = time(8, 14, 2);
    let text = "- [ ] Write tests";
    assert_eq!(
        apply_edits(text, set_done(text, 0, true, Some(now))),
        "- [x] Write tests @done (26-11-08 14:02)"
    );
}

#[test]
fn done_records_started_duration_and_clamps_negative_spans() {
    let started = time(7, 11, 32);
    let now = time(8, 14, 2);
    let text = "- [ ] Write tests @started(26-11-07 11:32)";
    assert_eq!(
        apply_edits(text, set_done(text, 0, true, Some(now))),
        "- [x] Write tests @started(26-11-07 11:32) @done (26-11-08 14:02) @lasted(1 day, 2:30)"
    );
    assert_eq!(stamp::lasted(time(1, 0, 0), time(4, 0, 0)), "3 days");
    assert_eq!(stamp::lasted(time(1, 0, 0), time(3, 0, 5)), "2 days, 0:05");
    let before_minute = NaiveDate::from_ymd_opt(2026, 11, 8)
        .unwrap()
        .and_hms_opt(14, 2, 59)
        .unwrap();
    let after_minute = NaiveDate::from_ymd_opt(2026, 11, 8)
        .unwrap()
        .and_hms_opt(14, 3, 0)
        .unwrap();
    assert_eq!(stamp::lasted(before_minute, after_minute), "0:01");
    assert_eq!(stamp::lasted(now, started), "0:00");
}

#[test]
fn cancel_replaces_done_and_lasted_stamps() {
    let now = time(8, 14, 2);
    let text = "- [x] Write tests @done (26-11-08 10:00) @lasted(4:02)";
    assert_eq!(
        apply_edits(text, set_state(text, 0, TaskState::Cancelled, Some(now))),
        "- [-] Write tests @cancelled (26-11-08 14:02)"
    );
}

#[test]
fn reopening_removes_stamps_but_keeps_personal_words_and_tags() {
    let now = time(8, 14, 2);
    let done = "- [x] Keep @done @done (26-11-08 10:00) @lasted(4:02)";
    assert_eq!(
        apply_edits(done, set_state(done, 0, TaskState::Open, Some(now))),
        "- [ ] Keep @done"
    );
    let cancelled = "- [-] Keep @done @tag @cancelled (26-11-08 10:00) @lasted(4:02)";
    assert_eq!(
        apply_edits(
            cancelled,
            set_state(cancelled, 0, TaskState::Open, Some(now))
        ),
        "- [ ] Keep @done @tag"
    );
}

#[test]
fn no_time_only_flips_the_checkbox() {
    let text = "- [ ] Keep @started(26-11-08 10:00)";
    assert_eq!(
        apply_edits(text, set_done(text, 0, true, None)),
        "- [x] Keep @started(26-11-08 10:00)"
    );
}

#[test]
fn same_state_is_a_no_op() {
    let now = time(8, 14, 2);
    let text = "- [x] Keep @done (26-11-08 10:00)";
    assert!(set_state(text, 0, TaskState::Done, Some(now)).is_empty());
    assert!(set_done(text, 0, true, None).is_empty());
}

#[test]
fn start_adds_once_and_does_not_change_state() {
    let now = time(8, 14, 2);
    let text = "- [x] Keep  ";
    let started = apply_edits(text, start(text, 0, now));
    assert_eq!(started, "- [x] Keep @started(26-11-08 14:02)  ");
    assert!(start(&started, 0, time(8, 15, 0)).is_empty());
    assert!(start("plain", 0, now).is_empty());
}

#[test]
fn scanner_accepts_both_written_spacing_forms() {
    let started = time(8, 14, 2);
    let label = "Keep @done(26-11-08 14:02) @started (26-11-08 14:02) @tag";
    assert_eq!(stamp::started_at(label), Some(started));
    assert_eq!(stamp::without_stamps(label), "Keep @tag");
    assert_eq!(stamp::spans(label, stamp::Tag::Done).len(), 1);
    assert_eq!(stamp::spans(label, stamp::Tag::Started).len(), 1);
}

#[test]
fn trailing_spaces_remain_after_a_new_stamp() {
    let now = time(8, 14, 2);
    let text = "- [ ] Keep \t ";
    assert_eq!(
        apply_edits(text, set_done(text, 0, true, Some(now))),
        "- [x] Keep @done (26-11-08 14:02) \t "
    );
}

#[test]
fn find_ignores_stamps_but_returns_raw_text() {
    let text = "- [ ] Write tests @started(26-11-08 10:00)\n";
    let found = find_in(text, "Write tests", None).unwrap();
    assert_eq!(found.text, "Write tests @started(26-11-08 10:00)");
}

#[test]
fn changes_report_state_and_start_events_for_stamped_edits() {
    let now = time(8, 14, 2);
    let old = "- [ ] Write tests\n";
    let checked = apply_edits(old, set_done(old, 0, true, Some(now)));
    let checked_changes = changes(old, &checked);
    assert_eq!(checked_changes[0].kind, ChangeKind::Checked);
    assert_eq!(
        checked_changes[0].text,
        "Write tests @done (26-11-08 14:02)"
    );
    assert!(
        !checked_changes
            .iter()
            .any(|change| change.kind == ChangeKind::Edited)
    );

    let reopened = apply_edits(&checked, set_state(&checked, 0, TaskState::Open, Some(now)));
    assert_eq!(changes(&checked, &reopened)[0].kind, ChangeKind::Unchecked);

    let started = apply_edits(old, start(old, 0, now));
    assert_eq!(changes(old, &started)[0].kind, ChangeKind::Started);
}

#[test]
fn returned_edits_are_sorted_and_apply_without_overlap() {
    let now = time(8, 14, 2);
    let text = "- [ ] Keep @cancelled(old) @done(old) @lasted(old)   ";
    let edits = set_state(text, 0, TaskState::Done, Some(now));
    assert!(
        edits
            .windows(2)
            .all(|pair| pair[0].range.end <= pair[1].range.start)
    );
    assert_eq!(
        Edit::apply_all(text, &edits),
        "- [x] Keep @done (26-11-08 14:02)   "
    );
}
