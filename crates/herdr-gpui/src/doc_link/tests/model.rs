use super::*;

const DEBOUNCE: Duration = Duration::from_secs(25);

#[test]
fn quiet_deadline_is_exact_at_the_boundary() {
    let now = base();
    let mut push = Push::new(version(1), DEBOUNCE);
    push.changed(version(2), false, now);
    assert_eq!(push.deadline(), Some(now + DEBOUNCE));
    assert!(
        push.take(now + Duration::from_millis(24_999), version(2), true)
            .is_none()
    );
    assert_eq!(
        push.take(now + DEBOUNCE, version(2), true),
        Some(Range {
            from: version(1),
            to: version(2),
        })
    );
}

#[test]
fn continued_typing_moves_one_deadline_outward() {
    let now = base();
    let mut push = Push::new(version(1), DEBOUNCE);
    push.changed(version(2), false, now);
    push.changed(version(3), false, now + Duration::from_secs(10));
    assert_eq!(push.deadline(), Some(now + Duration::from_secs(35)));
    assert!(
        push.take(now + Duration::from_secs(34), version(3), true)
            .is_none()
    );
}

#[test]
fn repeated_versions_are_ignored() {
    let now = base();
    let mut push = Push::new(version(1), DEBOUNCE);
    push.changed(version(2), false, now);
    push.changed(version(2), false, now + Duration::from_secs(10));
    assert_eq!(push.deadline(), Some(now + DEBOUNCE));
}

#[test]
fn self_edits_without_pending_work_advance_the_baseline() {
    let now = base();
    let mut push = Push::new(version(1), DEBOUNCE);
    push.changed(version(2), true, now);
    assert_eq!(push.acked(), version(2));
    assert_eq!(push.deadline(), None);
}

#[test]
fn self_edits_during_pending_work_keep_the_deadline() {
    let now = base();
    let mut push = Push::new(version(1), DEBOUNCE);
    push.changed(version(2), false, now);
    push.changed(version(3), true, now + Duration::from_secs(10));
    assert_eq!(push.deadline(), Some(now + DEBOUNCE));
    assert_eq!(
        push.take(now + DEBOUNCE, version(3), true),
        Some(Range {
            from: version(1),
            to: version(3),
        })
    );
}

#[test]
fn take_is_a_no_op_while_in_flight() {
    let now = base();
    let mut push = Push::new(version(1), DEBOUNCE);
    push.changed(version(2), false, now);
    assert!(push.take(now + DEBOUNCE, version(2), true).is_some());
    push.changed(version(3), false, now + DEBOUNCE);
    assert!(
        push.take(now + DEBOUNCE + DEBOUNCE, version(3), true)
            .is_none()
    );
    assert!(push.in_flight());
}

#[test]
fn sent_starts_the_next_range_at_the_previous_flight() {
    let now = base();
    let mut push = Push::new(version(1), DEBOUNCE);
    push.changed(version(2), false, now);
    assert!(push.take(now + DEBOUNCE, version(2), true).is_some());
    push.changed(version(3), false, now + DEBOUNCE);
    push.sent();
    assert_eq!(push.acked(), version(2));
    assert_eq!(
        push.take(now + DEBOUNCE * 2, version(3), true),
        Some(Range {
            from: version(2),
            to: version(3),
        })
    );
}

#[test]
fn failed_retries_the_old_range_with_newer_edits() {
    let now = base();
    let mut push = Push::new(version(1), DEBOUNCE);
    push.changed(version(2), false, now);
    assert!(push.take(now + DEBOUNCE, version(2), true).is_some());
    push.changed(version(3), false, now + DEBOUNCE);
    push.failed(now + DEBOUNCE, DEBOUNCE);
    assert_eq!(push.deadline(), Some(now + Duration::from_secs(50)));
    assert_eq!(
        push.take(now + Duration::from_secs(50), version(3), true),
        Some(Range {
            from: version(1),
            to: version(3),
        })
    );
}

#[test]
fn ready_again_removes_the_retry_delay() {
    let now = base();
    let mut push = Push::new(version(1), DEBOUNCE);
    push.changed(version(2), false, now);
    assert!(push.take(now + DEBOUNCE, version(2), true).is_some());
    push.failed(now + DEBOUNCE, DEBOUNCE);
    push.ready_again();
    assert_eq!(push.deadline(), Some(now + DEBOUNCE));
    assert_eq!(
        push.take(now + DEBOUNCE, version(2), true),
        Some(Range {
            from: version(1),
            to: version(2),
        })
    );
}

#[test]
fn ready_again_preserves_the_quiet_interval_for_newer_edits() {
    let now = base();
    let mut push = Push::new(version(1), DEBOUNCE);
    push.changed(version(2), false, now);
    assert!(push.take(now + DEBOUNCE, version(2), true).is_some());
    push.changed(version(3), false, now + DEBOUNCE);
    push.failed(now + DEBOUNCE, DEBOUNCE);
    push.ready_again();
    assert_eq!(push.deadline(), Some(now + DEBOUNCE * 2));
    assert!(push.take(now + DEBOUNCE, version(3), true).is_none());
}

#[test]
fn ineligible_work_is_acknowledged_without_a_flight() {
    let now = base();
    let mut push = Push::new(version(1), DEBOUNCE);
    push.changed(version(2), false, now);
    assert!(push.take(now + DEBOUNCE, version(2), false).is_none());
    assert_eq!(push.acked(), version(2));
    assert!(!push.in_flight());
    assert_eq!(push.deadline(), None);
}

#[test]
fn changing_debounce_moves_the_deadline() {
    let now = base();
    let mut push = Push::new(version(1), DEBOUNCE);
    push.changed(version(2), false, now);
    push.set_debounce(Duration::from_secs(5));
    assert_eq!(push.deadline(), Some(now + Duration::from_secs(5)));
}
