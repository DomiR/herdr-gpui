//! `doc` requests answered by the app, end to end over the control socket.
#![allow(clippy::unwrap_used, clippy::expect_used)]
use super::super::{State, serve as serve_request};
use super::*;
use crate::control::protocol::{Request, doc::TodoChangeKind};
use gpui::TestAppContext;

struct Doc {
    _dir: tempfile::TempDir,
    path: String,
}

fn file(text: &str) -> Doc {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("plan.md");
    std::fs::write(&path, text).unwrap();
    Doc {
        path: path.to_str().unwrap().to_owned(),
        _dir: dir,
    }
}

/// Sends `op` as pane `w_1:p1` and serves it until answered.
fn ask(op: DocOp, cx: &mut TestAppContext) -> Response {
    let address = socket::test_address();
    let server = socket::Server::bind(&address.path).unwrap();
    let path = address.path.clone();
    let request = Request::Doc(Call {
        caller: Caller {
            pane_id: Some("w_1:p1".into()),
            ..Caller::default()
        },
        op,
    });
    let caller = std::thread::spawn(move || socket::call(&path, &request));
    let incoming = server.next().unwrap();
    let mut state = State::default();
    cx.update(|cx| serve_request(incoming, &mut state, cx));
    // Until answered: a `--wait` request takes real seconds to expire.
    let until = std::time::Instant::now() + Duration::from_secs(10);
    while std::time::Instant::now() < until {
        if caller.is_finished() {
            break;
        }
        cx.run_until_parked();
        cx.update(|cx| state.watches.serve(cx));
        cx.executor().advance_clock(Duration::from_millis(10));
        std::thread::sleep(Duration::from_millis(1));
    }
    caller.join().unwrap().unwrap()
}

fn reply(response: Response) -> Reply {
    match response {
        Response::Doc { reply } => reply,
        other => panic!("expected a doc reply, got {other:?}"),
    }
}

#[gpui::test]
fn read_then_change_with_the_version_read(cx: &mut TestAppContext) {
    let doc = file("# Plan\n- [ ] one\n");
    let Reply::Text { version, text, .. } = reply(ask(
        DocOp::Read {
            path: doc.path.clone(),
            section: None,
        },
        cx,
    )) else {
        panic!("expected the text");
    };
    assert_eq!((version, text.as_str()), (1, "# Plan\n- [ ] one\n"));
    let updated = reply(ask(
        DocOp::TodoAdd {
            path: doc.path.clone(),
            version,
            section: None,
            text: "two".into(),
        },
        cx,
    ));
    assert!(
        matches!(
            updated,
            Reply::Updated {
                version: 2,
                others: None,
                ..
            }
        ),
        "{updated:?}"
    );
    // Written before the reply.
    assert_eq!(
        std::fs::read_to_string(&doc.path).unwrap(),
        "# Plan\n- [ ] one\n- [ ] two\n"
    );
    // The document closed with its last command; the next one carries on
    // counting from the same text.
    let checked = reply(ask(
        DocOp::TodoCheck {
            path: doc.path.clone(),
            version: 2,
            section: None,
            todo: "two".into(),
            done: true,
        },
        cx,
    ));
    assert!(
        matches!(checked, Reply::Updated { version: 3, .. }),
        "{checked:?}"
    );
}

#[gpui::test]
fn a_stale_version_is_refused_with_what_changed(cx: &mut TestAppContext) {
    let doc = file("# A\n- [ ] a\n# B\n- [ ] b\n");
    let first = reply(ask(
        DocOp::TodoAdd {
            path: doc.path.clone(),
            version: 1,
            section: Some("a".into()),
            text: "a2".into(),
        },
        cx,
    ));
    assert!(
        matches!(first, Reply::Updated { version: 2, .. }),
        "{first:?}"
    );
    // Another agent still at version 1, without a section: refused.
    let stale = reply(ask(
        DocOp::Append {
            path: doc.path.clone(),
            version: 1,
            section: None,
            text: "late".into(),
        },
        cx,
    ));
    assert!(
        matches!(
            stale,
            Reply::Mismatch {
                expected: 1,
                version: 2,
                ..
            }
        ),
        "{stale:?}"
    );
    // The app forgot version 1's place in history when the document closed,
    // so no diff; an agent then reads again.
    let (text, failed) = crate::control::client::doc_message(&stale);
    assert!(failed);
    assert!(
        text.starts_with("Version mismatch, nothing changed. Latest version: 2."),
        "{text}"
    );
    assert!(!std::fs::read_to_string(&doc.path).unwrap().contains("late"));
}

#[gpui::test]
fn requests_name_markdown_files_by_absolute_path(cx: &mut TestAppContext) {
    for path in ["relative.md", "/tmp/notes.txt"] {
        let response = ask(
            DocOp::Read {
                path: path.into(),
                section: None,
            },
            cx,
        );
        assert!(
            matches!(
                response,
                Response::Error {
                    code: ErrorCode::InvalidRequest,
                    ..
                }
            ),
            "{path}: {response:?}"
        );
    }
    let missing = ask(
        DocOp::Read {
            path: "/nonexistent/dir/plan.md".into(),
            section: None,
        },
        cx,
    );
    assert!(
        matches!(
            missing,
            Response::Error {
                code: ErrorCode::NotFound,
                ..
            }
        ),
        "{missing:?}"
    );
}

#[gpui::test]
fn a_change_that_changes_nothing_still_answers(cx: &mut TestAppContext) {
    let doc = file("- [x] done\n");
    let response = reply(ask(
        DocOp::TodoCheck {
            path: doc.path.clone(),
            version: 1,
            section: None,
            todo: "done".into(),
            done: true,
        },
        cx,
    ));
    assert!(
        matches!(response, Reply::Updated { version: 1, .. }),
        "{response:?}"
    );
}

#[gpui::test]
fn diff_waits_for_the_next_change(cx: &mut TestAppContext) {
    let doc = file("a\n");
    // Nothing changes: the wait runs out with an empty diff.
    let unchanged = reply(ask(
        DocOp::Diff {
            path: doc.path.clone(),
            since: 1,
            wait: 1,
            section: None,
            todos: false,
        },
        cx,
    ));
    assert!(
        matches!(&unchanged, Reply::Diff { diff: Some(diff), .. } if diff.is_empty()),
        "{unchanged:?}"
    );
    // Asked about an older version, it answers at once.
    let changed = reply(ask(
        DocOp::Diff {
            path: doc.path.clone(),
            since: 0,
            wait: 600,
            section: None,
            todos: false,
        },
        cx,
    ));
    assert!(
        matches!(changed, Reply::Diff { version: 1, .. }),
        "{changed:?}"
    );
}

/// Sends `op` from a thread and lets the test act on the app while it
/// waits; the answer.
fn ask_while(
    op: DocOp,
    cx: &mut TestAppContext,
    mut meanwhile: impl FnMut(&mut TestAppContext, usize),
) -> Response {
    let address = socket::test_address();
    let server = socket::Server::bind(&address.path).unwrap();
    let path = address.path.clone();
    let request = Request::Doc(Call {
        caller: Caller {
            pane_id: Some("w_1:p1".into()),
            ..Caller::default()
        },
        op,
    });
    let caller = std::thread::spawn(move || socket::call(&path, &request));
    let incoming = server.next().unwrap();
    let mut state = State::default();
    cx.update(|cx| serve_request(incoming, &mut state, cx));
    let until = std::time::Instant::now() + Duration::from_secs(10);
    let mut tick = 0;
    while std::time::Instant::now() < until && !caller.is_finished() {
        meanwhile(cx, tick);
        tick += 1;
        cx.run_until_parked();
        cx.update(|cx| state.watches.serve(cx));
        cx.executor().advance_clock(Duration::from_millis(100));
        std::thread::sleep(Duration::from_millis(1));
    }
    caller.join().unwrap().unwrap()
}

fn waiting_diff(path: &str, since: u64, section: Option<&str>, todos: bool) -> DocOp {
    DocOp::Diff {
        path: path.into(),
        since,
        wait: 30,
        section: section.map(str::to_owned),
        todos,
    }
}

#[gpui::test]
fn a_section_wait_skips_changes_elsewhere_and_names_who(cx: &mut TestAppContext) {
    let doc = file("# A\n- [ ] a\n# B\n- [ ] b\n");
    let path = doc.path.clone();
    let reply = reply(ask_while(
        waiting_diff(&doc.path, 1, Some("b"), true),
        cx,
        |_, tick| {
            // First a change in section A, which does not count, then one in B.
            if tick == 3 {
                std::fs::write(&path, "# A\n- [x] a\n# B\n- [ ] b\n").unwrap();
            }
            if tick == 20 {
                std::fs::write(&path, "# A\n- [x] a\n# B\n- [x] b\n").unwrap();
            }
        },
    ));
    let Reply::Diff {
        version,
        by,
        todos,
        diff,
        ..
    } = reply
    else {
        panic!("expected a diff");
    };
    assert_eq!(version, 3);
    assert_eq!(by, vec!["file".to_owned()]);
    assert_eq!(todos.len(), 1);
    assert_eq!(todos[0].kind, TodoChangeKind::Checked);
    assert_eq!(todos[0].number, 2);
    assert!(!diff.unwrap().contains("- [x] a"));
}

#[gpui::test]
fn a_watched_document_that_goes_away_answers_not_found(cx: &mut TestAppContext) {
    let doc = file("- [ ] a\n");
    let path = doc.path.clone();
    let response = ask_while(waiting_diff(&doc.path, 1, None, false), cx, |_, tick| {
        if tick == 3 {
            std::fs::remove_file(&path).unwrap();
        }
    });
    assert!(
        matches!(
            response,
            Response::Error {
                code: ErrorCode::NotFound,
                ..
            }
        ),
        "{response:?}"
    );
}

#[gpui::test]
fn an_agent_change_names_its_pane(cx: &mut TestAppContext) {
    let doc = file("- [ ] a\n");
    let added = reply(ask(
        DocOp::TodoAdd {
            path: doc.path.clone(),
            version: 1,
            section: None,
            text: "b".into(),
        },
        cx,
    ));
    assert!(matches!(added, Reply::Updated { version: 2, .. }));
    let Reply::Diff { by, .. } = reply(ask(
        DocOp::Diff {
            path: doc.path.clone(),
            since: 1,
            wait: 0,
            section: None,
            todos: false,
        },
        cx,
    )) else {
        panic!("expected a diff");
    };
    // The document closed after the change, so its history went with it;
    // within one session the agent is named by its pane.
    assert!(by.is_empty() || by == vec!["w_1:p1".to_owned()], "{by:?}");
}
