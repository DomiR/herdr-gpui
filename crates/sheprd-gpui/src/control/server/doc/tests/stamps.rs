use super::*;

use crate::control::protocol::doc::TaskState;

fn has_date_stamp(text: &str, prefix: &str) -> bool {
    let Some(start) = text.find(prefix).map(|start| start + prefix.len()) else {
        return false;
    };
    let Some(date) = text.as_bytes().get(start..start + 14) else {
        return false;
    };
    date.iter().enumerate().all(|(index, byte)| match index {
        2 | 5 => *byte == b'-',
        8 => *byte == b' ',
        11 => *byte == b':',
        _ => byte.is_ascii_digit(),
    }) && text.get(start + 14..start + 15) == Some(")")
}

#[gpui::test]
fn todo_state_changes_write_and_remove_stamps(cx: &mut TestAppContext) {
    let doc = file("- [ ] Ship\n");
    let checked = reply(ask(
        DocOp::TodoCheck {
            path: doc.path.clone(),
            version: 1,
            section: None,
            todo: "Ship".into(),
            done: true,
        },
        cx,
    ));
    assert!(matches!(checked, Reply::Updated { version: 2, .. }));
    let text = std::fs::read_to_string(&doc.path).unwrap();
    assert!(text.starts_with("- [x] Ship"));
    assert!(has_date_stamp(&text, " @done ("));

    let reopened = reply(ask(
        DocOp::TodoState {
            path: doc.path.clone(),
            version: 2,
            section: None,
            todo: "Ship".into(),
            state: TaskState::Open,
        },
        cx,
    ));
    assert!(matches!(reopened, Reply::Updated { version: 3, .. }));
    let text = std::fs::read_to_string(&doc.path).unwrap();
    assert!(text.starts_with("- [ ] Ship"));
    assert!(!text.contains("@done ("));

    let cancelled = reply(ask(
        DocOp::TodoState {
            path: doc.path.clone(),
            version: 3,
            section: None,
            todo: "Ship".into(),
            state: TaskState::Cancelled,
        },
        cx,
    ));
    assert!(matches!(cancelled, Reply::Updated { version: 4, .. }));
    let text = std::fs::read_to_string(&doc.path).unwrap();
    assert!(text.starts_with("- [-] Ship"));
    assert!(has_date_stamp(&text, " @cancelled ("));

    let started = reply(ask(
        DocOp::TodoStart {
            path: doc.path.clone(),
            version: 4,
            section: None,
            todo: "Ship".into(),
        },
        cx,
    ));
    assert!(matches!(started, Reply::Updated { version: 5, .. }));
    let text = std::fs::read_to_string(&doc.path).unwrap();
    assert!(text.starts_with("- [-] Ship"));
    assert!(has_date_stamp(&text, "@started("));
    assert_eq!(text.match_indices("@started(").count(), 1);

    let started_again = reply(ask(
        DocOp::TodoStart {
            path: doc.path.clone(),
            version: 5,
            section: None,
            todo: "Ship".into(),
        },
        cx,
    ));
    assert!(matches!(started_again, Reply::Updated { version: 5, .. }));
    let text_again = std::fs::read_to_string(&doc.path).unwrap();
    assert_eq!(text_again, text);

    let checked_again = reply(ask(
        DocOp::TodoCheck {
            path: doc.path,
            version: 5,
            section: None,
            todo: "Ship".into(),
            done: true,
        },
        cx,
    ));
    assert!(matches!(checked_again, Reply::Updated { version: 6, .. }));
    let text = std::fs::read_to_string(&doc.path).unwrap();
    assert!(text.starts_with("- [x] Ship"));
    assert!(has_date_stamp(&text, " @done ("));
    assert!(text.contains(" @lasted("));
}
