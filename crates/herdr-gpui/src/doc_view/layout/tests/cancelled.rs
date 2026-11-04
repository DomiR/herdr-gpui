use super::*;

#[test]
fn cancelled_tasks_are_struck_muted_and_keep_their_state() {
    let done_text = "- [x] done";
    let done_lines = markdown::lines(done_text);
    let done = row(done_text, &done_lines[0], &look(), false, &[], &|_| None);
    let cancelled_text = "- [-] cancelled";
    let cancelled_lines = markdown::lines(cancelled_text);
    let cancelled = row(
        cancelled_text,
        &cancelled_lines[0],
        &look(),
        false,
        &[],
        &|_| None,
    );
    let done_run = done.runs.last().unwrap();
    let cancelled_run = cancelled.runs.last().unwrap();
    assert_eq!(cancelled.checkbox, Some((2..5, TaskState::Cancelled)));
    assert_eq!(cancelled_run.color, done_run.color);
    assert_eq!(cancelled_run.strikethrough, done_run.strikethrough);
}
