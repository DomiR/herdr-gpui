//! The turns a continued conversation's fold hides are out of the text a
//! selection reads until the fold opens.

use super::*;
use sheprd_chat_protocol::SessionInfo;

fn user(id: u64, text: &str) -> Entry {
    Entry {
        id: EntryId(id),
        kind: EntryKind::User {
            files: 0,
            note: None,
            text: text.to_owned(),
        },
    }
}

fn continued() -> (Transcript, Prepared, Vec<(EntryId, u64)>) {
    let (mut transcript, _, keys) = prepare(vec![
        user(1, "folded prompt"),
        agent(2, "folded reply"),
        user(3, "last prompt"),
        agent(4, "last reply"),
        Entry {
            id: EntryId(5),
            kind: EntryKind::Notice {
                answers: Vec::new(),
                details: None,
                level: NoticeLevel::Info,
                text: "Continued".into(),
            },
        },
    ]);
    transcript.session = Some(SessionInfo {
        continued: Some(EntryId(5)),
        cwd: None,
        id: "s".into(),
        title: None,
    });
    let mut prepared = Prepared::default();
    prepared.update(&transcript, &keys);
    (transcript, prepared, keys)
}

#[test]
fn select_all_skips_folded_turns_until_the_fold_opens() {
    let (transcript, mut prepared, keys) = continued();
    let mut selection = Selection::default();
    selection.select_all(prepared.text());
    let text = selection.text(prepared.text()).unwrap();
    assert!(!text.contains("folded"), "{text}");
    assert!(text.starts_with("last prompt"), "{text}");

    prepared.set_expanded(&std::collections::HashSet::from([EntryId(5)]));
    prepared.update(&transcript, &keys);
    selection.select_all(prepared.text());
    let text = selection.text(prepared.text()).unwrap();
    assert!(text.starts_with("folded prompt"), "{text}");
}

#[test]
fn folding_ends_a_selection_inside_the_folded_turns() {
    let (transcript, mut prepared, keys) = continued();
    prepared.set_expanded(&std::collections::HashSet::from([EntryId(5)]));
    prepared.update(&transcript, &keys);
    let mut selection = Selection::default();
    selection.select_all(prepared.text());
    // `validate` says whether it cleared a stale selection.
    assert!(!selection.validate(prepared.text()));

    prepared.set_expanded(&std::collections::HashSet::new());
    prepared.update(&transcript, &keys);
    assert!(selection.validate(prepared.text()));
    assert_eq!(selection.text(prepared.text()), None);
}
