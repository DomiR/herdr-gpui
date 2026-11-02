#![allow(clippy::unwrap_used)]
use super::*;
use std::io;

fn busy() -> crate::Error {
    crate::control::Error::Rejected {
        code: ErrorCode::Busy,
        message: "busy".into(),
    }
    .into()
}

fn interrupted() -> crate::Error {
    crate::control::Error::Io(io::Error::from(io::ErrorKind::BrokenPipe)).into()
}

#[test]
fn busy_is_retried_with_backoff_and_the_same_cursor() {
    let mut app = App::new(vec![
        sections(3),
        Err(busy()),
        Err(busy()),
        Err(busy()),
        diff(3, 4, &["the user"], Some("@@\n+x\n")),
    ]);
    let (out, status, pauses) = run_app(&watch(None, false, false), &mut app);

    assert!(out.contains("Version 4 by the user\n@@\n+x\n"));
    assert!(out.contains("TODO.md was deleted or moved; stopped watching"));
    assert!(!out.contains("restarted"));
    assert_eq!(status, exit::GONE);
    assert_eq!(
        pauses,
        vec![
            Duration::from_millis(100),
            Duration::from_millis(200),
            Duration::from_millis(400),
        ]
    );

    let asked = app.asked.borrow();
    let diffs: Vec<_> = asked
        .iter()
        .filter_map(|op| match op {
            DocOp::Diff {
                since,
                section,
                todos,
                ..
            } => Some((*since, section.as_deref(), *todos)),
            _ => None,
        })
        .collect();
    assert_eq!(diffs.len(), 5);
    assert!(
        diffs[..4]
            .iter()
            .all(|diff| *diff == (3, Some("next"), false))
    );
    assert_eq!(diffs[4], (4, Some("next"), false));
}

#[test]
fn busy_while_looking_up_the_version_is_retried() {
    let mut app = App::new(vec![Err(busy()), Err(busy()), sections(3)]);
    let (out, status, pauses) = run_app(&watch(None, false, false), &mut app);

    assert!(out.starts_with("Watching TODO.md at version 3\n"));
    assert_eq!(status, exit::GONE);
    assert_eq!(
        pauses,
        vec![Duration::from_millis(100), Duration::from_millis(200)]
    );
    assert!(
        app.asked
            .borrow()
            .iter()
            .take(3)
            .all(|op| matches!(op, DocOp::Sections { .. }))
    );
}

#[test]
fn an_io_error_keeps_the_cursor() {
    let mut app = App::new(vec![
        Err(interrupted()),
        diff(3, 5, &["file"], Some("@@\n+x\n")),
    ]);
    let (out, status, pauses) = run_app(&watch(Some(3), false, false), &mut app);

    assert!(out.contains("Versions 4-5 by file\n@@\n+x\n"));
    assert!(!out.contains("restarted"));
    assert_eq!(status, exit::GONE);
    assert_eq!(pauses, vec![Duration::from_millis(100)]);
    let asked = app.asked.borrow();
    assert!(asked.iter().all(|op| matches!(op, DocOp::Diff { .. })));
    let sinces: Vec<_> = asked
        .iter()
        .filter_map(|op| match op {
            DocOp::Diff { since, .. } => Some(*since),
            _ => None,
        })
        .collect();
    assert_eq!(sinces, vec![3, 3, 5]);
}

#[test]
fn json_busy_retries_without_printing_an_event() {
    let mut app = App::new(vec![Err(busy()), diff(1, 2, &["file"], Some("@@\n+x\n"))]);
    let (out, _, _) = run_app(&watch(Some(1), false, true), &mut app);
    let lines: Vec<serde_json::Value> = out
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();

    assert_eq!(lines.len(), 2);
    assert_eq!(lines[0]["event"], "change");
    assert_eq!(lines[1]["event"], "gone");
}
