//! A chat's log, mirrored, shows what the person wrote, not Sheprd's notes.

use super::*;
use crate::context::{Context, Facts, Prompts};

fn said(text: &str, note: Option<&str>) -> Entry {
    Entry {
        id: EntryId(1),
        kind: EntryKind::User {
            text: text.to_owned(),
            files: 0,
            note: note.map(str::to_owned),
        },
    }
}

fn note() -> String {
    Context::new(
        Prompts::with_instructions("Be brief."),
        Facts::default(),
        Default::default(),
    )
    .before(None)
    .unwrap()
}

#[test]
fn notes_sent_to_the_agent_stay_hidden_in_a_mirror() {
    let with_note = |words: &str| {
        let mut transcript = Transcript::new("Claude".to_owned());
        transcript.entries = vec![said(&format!("{}\n{words}", note()), None)];
        transcript
    };
    let note = note();
    let mut mirror = Mirror::new(with_note("first"), None);
    assert_eq!(mirror.transcript.entries, [said("first", Some(&note))]);
    mirror.update(with_note("edited"));
    assert_eq!(mirror.transcript.entries, [said("edited", Some(&note))]);
}
