use super::*;

#[test]
fn running_summaries_are_bounded() {
    let slots: Vec<Slot> = (0..MAX_RUNNING).map(|_| Slot::take().unwrap()).collect();
    assert!(matches!(Slot::take(), Err(Error::Busy)));
    drop(slots);
    assert!(Slot::take().is_ok());
}

#[test]
fn failures_answer_with_their_own_status() {
    use axum::http::StatusCode;
    assert_eq!(Error::Busy.status(), StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(
        Error::from(one_shot::Error::NoAgent).status(),
        StatusCode::CONFLICT
    );
    assert_eq!(
        Error::from(one_shot::Error::TimedOut).status(),
        StatusCode::GATEWAY_TIMEOUT
    );
    assert_eq!(
        Error::from(one_shot::Error::Empty).status(),
        StatusCode::BAD_GATEWAY
    );
}
