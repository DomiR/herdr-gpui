use super::*;

#[test]
fn list_prints_cancelled_checkbox() {
    let reply = Reply::Todos {
        path: "/a.md".into(),
        version: 3,
        todos: vec![TodoInfo {
            number: 1,
            line: 2,
            done: false,
            state: TaskState::Cancelled,
            text: "ship".into(),
            depth: 0,
            section: None,
            tags: Vec::new(),
        }],
    };
    assert_eq!(message(&reply).0, "Version: 3\n1. [-] ship");
}
