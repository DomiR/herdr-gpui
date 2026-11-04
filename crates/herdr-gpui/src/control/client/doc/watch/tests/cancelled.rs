use super::*;

#[test]
fn prints_cancelled_and_uncancelled_lines() {
    let body = Body::Todos(vec![
        TodoChangeInfo {
            kind: TodoChangeKind::Cancelled,
            number: 3,
            text: "Ship".into(),
        },
        TodoChangeInfo {
            kind: TodoChangeKind::Uncancelled,
            number: 3,
            text: "Ship".into(),
        },
    ]);
    assert_eq!(
        change_text(2, 3, &[], &body),
        "Version 3\ncancelled 3. Ship\nuncancelled 3. Ship"
    );
}
