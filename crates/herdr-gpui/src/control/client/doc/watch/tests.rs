#![allow(clippy::unwrap_used)]
use super::*;
use crate::control::protocol::doc::TodoChangeKind;
use std::{cell::RefCell, collections::VecDeque, path::PathBuf};

/// A fake app: answers each request with the next scripted answer, and
/// remembers what was asked. When the script runs out, the document goes.
struct App {
    answers: VecDeque<crate::Result<Response>>,
    asked: RefCell<Vec<DocOp>>,
}

impl App {
    fn new(answers: Vec<crate::Result<Response>>) -> Self {
        Self {
            answers: answers.into(),
            asked: RefCell::default(),
        }
    }

    fn ask(&mut self, op: DocOp) -> crate::Result<Response> {
        self.asked.borrow_mut().push(op);
        self.answers
            .pop_front()
            .unwrap_or_else(|| Err(gone_error()))
    }
}

fn gone_error() -> crate::Error {
    crate::control::Error::Rejected {
        code: ErrorCode::NotFound,
        message: "gone".into(),
    }
    .into()
}

fn away() -> crate::Error {
    crate::control::Error::Unavailable {
        path: PathBuf::from("/tmp/control.sock"),
        source: std::io::Error::from(std::io::ErrorKind::ConnectionRefused),
    }
    .into()
}

fn sections(version: u64) -> crate::Result<Response> {
    Ok(Response::Doc {
        reply: Reply::Sections {
            path: "/a/TODO.md".into(),
            version,
            sections: Vec::new(),
        },
    })
}

fn diff(since: u64, version: u64, by: &[&str], text: Option<&str>) -> crate::Result<Response> {
    Ok(Response::Doc {
        reply: Reply::Diff {
            path: "/a/TODO.md".into(),
            since,
            version,
            diff: text.map(str::to_owned),
            by: by.iter().map(|who| (*who).to_owned()).collect(),
            todos: Vec::new(),
        },
    })
}

fn watch(since: Option<u64>, todos: bool, json: bool) -> Watch {
    Watch {
        path: "/a/TODO.md".into(),
        shown: "TODO.md".into(),
        since,
        section: Some("next".into()),
        todos,
        json,
    }
}

fn run_app(watch: &Watch, app: &mut App) -> (String, u8, Vec<Duration>) {
    let mut out = Vec::new();
    let mut pauses = Vec::new();
    let status = run(watch, &mut |op| app.ask(op), &mut out, &mut |pause| {
        pauses.push(pause)
    })
    .unwrap();
    (String::from_utf8(out).unwrap(), status, pauses)
}

#[test]
fn prints_each_change_under_its_header_until_the_document_is_gone() {
    let mut app = App::new(vec![
        sections(4),
        // No change in the wait: nothing printed.
        diff(4, 4, &[], Some("")),
        diff(
            4,
            5,
            &["the user"],
            Some("--- 4\n+++ 5\n@@ -1 +1 @@\n-a\n+b\n"),
        ),
        // Two versions landed at once: one combined change.
        diff(
            5,
            7,
            &["claude (w1:p2)", "file"],
            Some("@@ -1 +1 @@\n-b\n+c\n"),
        ),
    ]);
    let (out, status, _) = run_app(&watch(None, false, false), &mut app);
    assert_eq!(
        out,
        "Watching TODO.md at version 4\n\
         Version 5 by the user\n--- 4\n+++ 5\n@@ -1 +1 @@\n-a\n+b\n\
         Versions 6-7 by claude (w1:p2), file\n@@ -1 +1 @@\n-b\n+c\n\
         TODO.md was deleted or moved; stopped watching\n"
    );
    assert_eq!(status, exit::GONE);
    // Every wait asks from the last version printed, for its section.
    let asked = app.asked.borrow();
    let sinces: Vec<(u64, Option<String>)> = asked
        .iter()
        .filter_map(|op| match op {
            DocOp::Diff { since, section, .. } => Some((*since, section.clone())),
            _ => None,
        })
        .collect();
    assert_eq!(
        sinces,
        vec![
            (4, Some("next".into())),
            (4, Some("next".into())),
            (5, Some("next".into())),
            (7, Some("next".into())),
        ]
    );
}

#[test]
fn tasks_print_one_line_each() {
    let todos = vec![
        TodoChangeInfo {
            kind: TodoChangeKind::Checked,
            number: 3,
            text: "Warn when low".into(),
        },
        TodoChangeInfo {
            kind: TodoChangeKind::Added,
            number: 12,
            text: "Ship".into(),
        },
    ];
    let mut app = App::new(vec![Ok(Response::Doc {
        reply: Reply::Diff {
            path: "/a/TODO.md".into(),
            since: 2,
            version: 3,
            diff: Some("@@".into()),
            by: vec!["the user".into()],
            todos,
        },
    })]);
    let (out, _, _) = run_app(&watch(Some(2), true, false), &mut app);
    assert_eq!(
        out,
        "Version 3 by the user\nchecked 3. Warn when low\nadded 12. Ship\n\
         TODO.md was deleted or moved; stopped watching\n"
    );
    assert!(matches!(
        app.asked.borrow()[0],
        DocOp::Diff {
            todos: true,
            wait: 600,
            ..
        }
    ));
}

#[test]
fn json_prints_one_object_per_line() {
    let mut app = App::new(vec![diff(1, 2, &["file"], Some("@@\n+x\n"))]);
    let (out, _, _) = run_app(&watch(Some(1), false, true), &mut app);
    let lines: Vec<serde_json::Value> = out
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[0]["event"], "change");
    assert_eq!(lines[0]["since"], 1);
    assert_eq!(lines[0]["version"], 2);
    assert_eq!(lines[0]["by"][0], "file");
    assert_eq!(lines[0]["diff"], "@@\n+x\n");
    assert_eq!(lines[1]["event"], "gone");
}

#[test]
fn an_app_restart_reconnects_with_one_line() {
    let mut app = App::new(vec![
        Err(away()),
        // While it is away, the version is asked for again each second.
        Err(away()),
        Err(away()),
        sections(1),
        diff(1, 2, &["the user"], Some("@@\n+y\n")),
    ]);
    let (out, status, pauses) = run_app(&watch(Some(9), false, false), &mut app);
    assert_eq!(
        out,
        "Herdr UI restarted; watching TODO.md at version 1\n\
         Version 2 by the user\n@@\n+y\n\
         TODO.md was deleted or moved; stopped watching\n"
    );
    assert_eq!(status, exit::GONE);
    assert_eq!(pauses.len(), 2);
}

#[test]
fn a_document_already_gone_ends_at_once() {
    let mut app = App::new(vec![]);
    let (out, status, _) = run_app(&watch(None, false, false), &mut app);
    assert_eq!(out, "TODO.md was deleted or moved; stopped watching\n");
    assert_eq!(status, exit::GONE);
}

#[test]
fn a_change_too_large_to_show_still_moves_on() {
    let mut app = App::new(vec![diff(3, 5, &["file"], None)]);
    let (out, _, _) = run_app(&watch(Some(3), false, false), &mut app);
    assert!(out.starts_with("Versions 4-5 by file\n(the diff is too large; read the document)\n"));
}
