use super::*;
use sheprd_chat_protocol::{Entry, NoticeLevel, SessionInfo};

fn entry(id: u64, kind: EntryKind) -> Entry {
    Entry {
        id: EntryId(id),
        kind,
    }
}

fn user(id: u64) -> Entry {
    entry(
        id,
        EntryKind::User {
            files: 0,
            note: None,
            text: format!("prompt {id}"),
        },
    )
}

fn agent(id: u64) -> Entry {
    entry(
        id,
        EntryKind::Agent {
            text: format!("reply {id}"),
        },
    )
}

fn divider(id: u64) -> Entry {
    entry(
        id,
        EntryKind::Notice {
            answers: Vec::new(),
            details: None,
            level: NoticeLevel::Info,
            text: "Continued".into(),
        },
    )
}

fn transcript(entries: Vec<Entry>, continued: Option<u64>) -> Transcript {
    Transcript {
        entries,
        session: Some(SessionInfo {
            continued: continued.map(EntryId),
            cwd: None,
            id: "s".into(),
            title: None,
        }),
        ..Transcript::new("Claude")
    }
}

#[test]
fn folds_everything_before_the_brought_conversations_last_prompt() {
    let transcript = transcript(
        vec![
            user(1),
            agent(2),
            user(3),
            agent(4),
            user(5),
            agent(6),
            divider(7),
            user(8),
        ],
        Some(7),
    );
    let fold = earlier(&transcript, &HashSet::new()).unwrap();
    assert_eq!(
        fold,
        Earlier {
            divider: EntryId(7),
            open: false,
            start: 4,
            turns: 2,
        }
    );
    let hidden: Vec<_> = (0..transcript.entries.len())
        .filter(|index| fold.hides(*index))
        .collect();
    assert_eq!(hidden, [0, 1, 2, 3]);
}

#[test]
fn an_opened_fold_hides_nothing() {
    let transcript = transcript(vec![user(1), agent(2), user(3), divider(4)], Some(4));
    let fold = earlier(&transcript, &HashSet::from([EntryId(4)])).unwrap();
    assert!(fold.open);
    assert!((0..4).all(|index| !fold.hides(index)));
}

#[test]
fn nothing_folds_without_a_divider_or_anything_before_the_last_prompt() {
    let entries = vec![user(1), agent(2), divider(3)];
    assert_eq!(
        earlier(&transcript(entries.clone(), None), &HashSet::new()),
        None
    );
    assert_eq!(
        earlier(&transcript(entries, Some(3)), &HashSet::new()),
        None
    );
    // A divider dropped to keep the transcript bounded folds nothing.
    let entries = vec![user(1), agent(2), user(3)];
    assert_eq!(
        earlier(&transcript(entries, Some(9)), &HashSet::new()),
        None
    );
}
