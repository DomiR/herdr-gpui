#![allow(clippy::unwrap_used)]
use super::*;
use std::collections::VecDeque;

fn busy() -> crate::Error {
    crate::control::Error::Rejected {
        code: ErrorCode::Busy,
        message: "busy".into(),
    }
    .into()
}

fn answer() -> Response {
    Response::Doc {
        reply: Reply::Diff {
            path: "/a.md".into(),
            since: 4,
            version: 5,
            diff: Some("@@\n+x\n".into()),
            by: vec!["file".into()],
            todos: Vec::new(),
        },
    }
}

#[test]
fn busy_spends_the_wait_and_asks_for_what_is_left() {
    let mut answers = VecDeque::from([Err(busy()), Err(busy()), Ok(answer())]);
    let mut asked = Vec::new();
    let mut pauses = Vec::new();
    let result = diff_waiting(
        10,
        &mut |wait| {
            asked.push(wait);
            answers.pop_front().unwrap()
        },
        &mut |delay| pauses.push(delay),
    );

    assert!(matches!(result, Ok(Response::Doc { .. })));
    assert_eq!(asked, vec![10, 9, 9]);
    assert_eq!(
        pauses,
        vec![Duration::from_millis(100), Duration::from_millis(200)]
    );
}

#[test]
fn busy_until_the_wait_is_spent_ends_busy() {
    let mut asked = Vec::new();
    let mut pauses = Vec::new();
    let result = diff_waiting(
        1,
        &mut |wait| {
            asked.push(wait);
            Err(busy())
        },
        &mut |delay| pauses.push(delay),
    );
    assert!(is_busy(&result.unwrap_err()));
    assert_eq!(asked, vec![1]);
    assert!(pauses.is_empty());

    asked.clear();
    pauses.clear();
    let result = diff_waiting(
        3,
        &mut |wait| {
            asked.push(wait);
            Err(busy())
        },
        &mut |delay| pauses.push(delay),
    );
    assert!(is_busy(&result.unwrap_err()));
    assert!(pauses.iter().copied().sum::<Duration>() < Duration::from_secs(3));
    assert!(asked.iter().all(|wait| *wait > 0));
}

#[test]
fn zero_wait_asks_once() {
    let mut asked = Vec::new();
    let mut pauses = Vec::new();
    let result = diff_waiting(
        0,
        &mut |wait| {
            asked.push(wait);
            Ok(answer())
        },
        &mut |delay| pauses.push(delay),
    );

    assert!(matches!(result, Ok(Response::Doc { .. })));
    assert_eq!(asked, vec![0]);
    assert!(pauses.is_empty());
}
