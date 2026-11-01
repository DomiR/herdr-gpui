//! `doc` requests answered by the app, end to end over the control socket.
#![allow(clippy::unwrap_used, clippy::expect_used)]
use super::super::{State, serve as serve_request};
use super::*;
use crate::control::protocol::Request;
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
                code: ErrorCode::Failed,
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
        },
        cx,
    ));
    assert!(
        matches!(changed, Reply::Diff { version: 1, .. }),
        "{changed:?}"
    );
}
