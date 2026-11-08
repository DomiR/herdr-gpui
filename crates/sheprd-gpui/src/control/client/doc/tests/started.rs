use super::*;

#[test]
fn list_keeps_started_stamp_in_task_text() {
    let reply = Reply::Todos {
        path: "/a.md".into(),
        version: 3,
        todos: vec![TodoInfo {
            number: 1,
            line: 2,
            done: false,
            state: TaskState::Open,
            text: "Ship @started(26-11-08 14:02)".into(),
            depth: 0,
            section: None,
        }],
    };
    assert_eq!(
        message(&reply).0,
        "Version: 3\n1. [ ] Ship @started(26-11-08 14:02)"
    );
}
