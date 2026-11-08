use super::*;

#[gpui::test]
fn list_filters_by_any_tag_and_keeps_document_numbers(cx: &mut TestAppContext) {
    let doc = file("# Work\n- [ ] low @low\n- [ ] high @High\n\n# Later\n- [ ] due @due\n");
    let listed = reply(ask(
        DocOp::TodoList {
            path: doc.path,
            section: None,
            tags: vec!["high".into(), "@DUE".into()],
        },
        cx,
    ));
    let Reply::Todos { todos, .. } = listed else {
        panic!("expected todo list");
    };
    assert_eq!(
        todos.iter().map(|todo| todo.number).collect::<Vec<_>>(),
        vec![2, 3]
    );
    assert_eq!(todos[0].tags, vec!["high"]);
    assert_eq!(todos[1].tags, vec!["due"]);

    let section_doc = file("# Work\n- [ ] low @low\n\n# Later\n- [ ] due @due\n");
    let listed = reply(ask(
        DocOp::TodoList {
            path: section_doc.path,
            section: Some("later".into()),
            tags: vec!["due".into()],
        },
        cx,
    ));
    let Reply::Todos { todos, .. } = listed else {
        panic!("expected todo list");
    };
    assert_eq!(
        todos.iter().map(|todo| todo.number).collect::<Vec<_>>(),
        vec![2]
    );
}

#[gpui::test]
fn list_refuses_invalid_tags(cx: &mut TestAppContext) {
    let doc = file("- [ ] task\n");
    let response = ask(
        DocOp::TodoList {
            path: doc.path,
            section: None,
            tags: vec!["not.valid".into()],
        },
        cx,
    );
    assert!(matches!(response, Response::Error { .. }));
}
