use super::*;

#[test]
fn prints_started_task_lines() {
    let body = Body::Todos(vec![TodoChangeInfo {
        kind: TodoChangeKind::Started,
        number: 3,
        text: "Ship".into(),
    }]);
    assert_eq!(change_text(2, 3, &[], &body), "Version 3\nstarted 3. Ship");
}
