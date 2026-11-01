#![allow(clippy::unwrap_used)]
use super::*;
use crate::control::protocol::doc::{Others, TodoInfo};

#[test]
fn replies_read_as_one_actionable_message() {
    let updated = Reply::Updated {
        path: "/a.md".into(),
        version: 7,
        section: None,
        others: None,
    };
    assert_eq!(message(&updated), ("Updated. Version: 7".into(), false));
    let section = Reply::Updated {
        path: "/a.md".into(),
        version: 7,
        section: Some("goals".into()),
        others: Some(Others {
            since: 5,
            diff: Some("@@ -1 +1 @@\n-a\n+b\n".into()),
        }),
    };
    assert_eq!(
        message(&section).0,
        "Updated section \"goals\". Version: 7. Other sections changed since 5:\n@@ -1 +1 @@\n-a\n+b\n"
    );
    let mismatch = Reply::Mismatch {
        path: "/a.md".into(),
        expected: 5,
        version: 7,
        diff: Some("@@\n+x\n".into()),
    };
    assert_eq!(
        message(&mismatch),
        (
            "Version mismatch, nothing changed. Latest version: 7. Changes since 5:\n@@\n+x\n"
                .into(),
            true
        )
    );
    let forgotten = Reply::Mismatch {
        path: "/a.md".into(),
        expected: 1,
        version: 7,
        diff: None,
    };
    assert!(
        message(&forgotten)
            .0
            .ends_with("Run herdr-gpui doc read /a.md for the current text.")
    );
}

#[test]
fn todos_and_reads_start_with_the_version() {
    let todos = Reply::Todos {
        path: "/a.md".into(),
        version: 3,
        todos: vec![TodoInfo {
            number: 1,
            line: 2,
            done: true,
            text: "ship".into(),
            depth: 2,
            section: None,
        }],
    };
    assert_eq!(message(&todos).0, "Version: 3\n  1. [x] ship");
    let read = Reply::Text {
        path: "/a.md".into(),
        version: 3,
        section: None,
        text: "# A\n".into(),
    };
    assert_eq!(message(&read).0, "Version: 3\n\n# A\n");
}

#[test]
fn relative_paths_resolve_here() {
    let path = absolute(Path::new("notes/todo.md")).unwrap();
    assert!(Path::new(&path).is_absolute());
    assert!(path.ends_with("notes/todo.md"));
}
