use super::*;

#[test]
fn parses_cancelled_and_rejects_tilde() {
    assert_eq!(
        lines("- [-] cancelled")[0].task(),
        Some(TaskState::Cancelled)
    );
    assert_eq!(lines("- [~] plain")[0].task(), None);
}
