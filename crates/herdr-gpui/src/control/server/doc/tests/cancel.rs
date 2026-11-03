use super::*;

use crate::control::protocol::doc::TaskState;

#[gpui::test]
fn cancel_uncancel_and_check_use_typed_states(cx: &mut TestAppContext) {
    let doc = file("- [ ] ship\n");
    let cancelled = reply(ask(
        DocOp::TodoState {
            path: doc.path.clone(),
            version: 1,
            section: None,
            todo: "ship".into(),
            state: TaskState::Cancelled,
        },
        cx,
    ));
    assert!(matches!(cancelled, Reply::Updated { version: 2, .. }));

    let listed = reply(ask(
        DocOp::TodoList {
            path: doc.path.clone(),
            section: None,
        },
        cx,
    ));
    let Reply::Todos { todos, .. } = listed else {
        panic!("expected todo list");
    };
    assert_eq!(todos[0].state, TaskState::Cancelled);
    assert!(!todos[0].done);

    let checked = reply(ask(
        DocOp::TodoCheck {
            path: doc.path.clone(),
            version: 2,
            section: None,
            todo: "ship".into(),
            done: true,
        },
        cx,
    ));
    assert!(matches!(checked, Reply::Updated { version: 3, .. }));

    let cancelled_again = reply(ask(
        DocOp::TodoState {
            path: doc.path.clone(),
            version: 3,
            section: None,
            todo: "ship".into(),
            state: TaskState::Cancelled,
        },
        cx,
    ));
    assert!(matches!(cancelled_again, Reply::Updated { version: 4, .. }));

    let reopened = reply(ask(
        DocOp::TodoState {
            path: doc.path.clone(),
            version: 4,
            section: None,
            todo: "ship".into(),
            state: TaskState::Open,
        },
        cx,
    ));
    assert!(matches!(reopened, Reply::Updated { version: 5, .. }));

    let listed = reply(ask(
        DocOp::TodoList {
            path: doc.path.clone(),
            section: None,
        },
        cx,
    ));
    let Reply::Todos { todos, .. } = listed else {
        panic!("expected todo list");
    };
    assert_eq!(todos[0].state, TaskState::Open);
    assert!(!todos[0].done);
}

#[gpui::test]
fn cancelling_a_stale_version_is_refused(cx: &mut TestAppContext) {
    let doc = file("- [ ] ship\n");
    let changed = reply(ask(
        DocOp::TodoState {
            path: doc.path.clone(),
            version: 1,
            section: None,
            todo: "ship".into(),
            state: TaskState::Cancelled,
        },
        cx,
    ));
    assert!(matches!(changed, Reply::Updated { version: 2, .. }));
    let stale = reply(ask(
        DocOp::TodoState {
            path: doc.path,
            version: 1,
            section: None,
            todo: "ship".into(),
            state: TaskState::Open,
        },
        cx,
    ));
    assert!(matches!(stale, Reply::Mismatch { expected: 1, version: 2, .. }));
}
