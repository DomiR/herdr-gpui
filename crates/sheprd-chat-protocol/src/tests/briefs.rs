use serde_json::json;

use super::*;

fn mark(transcript: &mut Transcript, entry: u64, from: Option<Agent>) -> Result<()> {
    transcript.apply(&Change::Brief {
        brief: Brief {
            entry: EntryId(entry),
            from,
        },
    })
}

#[test]
fn a_brief_marks_a_prompt_already_pushed() {
    let mut transcript = Transcript::new("Claude");
    transcript
        .apply(&Change::Push {
            entry: user(1, "Brief"),
        })
        .unwrap();
    mark(&mut transcript, 1, Some(Agent::Claude)).unwrap();
    assert_eq!(
        transcript.brief(EntryId(1)).unwrap().from,
        Some(Agent::Claude)
    );
    // Marked again: replaced, not kept twice.
    mark(&mut transcript, 1, None).unwrap();
    assert_eq!(
        transcript.briefs,
        [Brief {
            entry: EntryId(1),
            from: None
        }]
    );
}

#[test]
fn only_a_prompt_can_be_a_brief() {
    let mut transcript = Transcript::new("Claude");
    transcript
        .apply(&Change::Push {
            entry: agent(1, "Answer"),
        })
        .unwrap();
    let error = mark(&mut transcript, 1, None).unwrap_err();
    assert!(matches!(
        error,
        Error::Brief(brief::Error::NotPrompt(EntryId(1)))
    ));
    let error = mark(&mut transcript, 2, None).unwrap_err();
    assert!(matches!(
        error,
        Error::Brief(brief::Error::NotPrompt(EntryId(2)))
    ));
    assert!(transcript.briefs.is_empty());
}

#[test]
fn briefs_are_bounded() {
    let mut transcript = Transcript::new("Claude");
    for id in 1..=MAX_BRIEFS as u64 + 1 {
        transcript
            .apply(&Change::Push {
                entry: user(id, "Brief"),
            })
            .unwrap();
    }
    for id in 1..=MAX_BRIEFS as u64 {
        mark(&mut transcript, id, None).unwrap();
    }
    let error = mark(&mut transcript, MAX_BRIEFS as u64 + 1, None).unwrap_err();
    assert!(matches!(error, Error::Brief(brief::Error::Limit)));
    assert_eq!(transcript.briefs.len(), MAX_BRIEFS);
}

#[test]
fn truncating_drops_the_briefs_of_dropped_entries() {
    let mut transcript = Transcript::new("Claude");
    for id in 1..=3 {
        transcript
            .apply(&Change::Push {
                entry: user(id, "Brief"),
            })
            .unwrap();
        mark(&mut transcript, id, None).unwrap();
    }
    transcript
        .apply(&Change::Truncate { before: EntryId(3) })
        .unwrap();
    assert_eq!(
        transcript.briefs,
        [Brief {
            entry: EntryId(3),
            from: None
        }]
    );
}

#[test]
fn a_transcript_without_briefs_reads_and_writes_as_before() {
    let transcript = Transcript::new("Claude");
    let value = serde_json::to_value(&transcript).unwrap();
    assert!(value.get("briefs").is_none());
    let older: Transcript = serde_json::from_value(json!({"agent": "Claude"})).unwrap();
    assert!(older.briefs.is_empty());
}

#[test]
fn a_brief_has_a_stable_wire_form() {
    let change = Change::Brief {
        brief: Brief {
            entry: EntryId(7),
            from: Some(Agent::Codex),
        },
    };
    let value = serde_json::to_value(&change).unwrap();
    assert_eq!(
        value,
        json!({"change": "brief", "brief": {"entry": 7, "from": "codex"}})
    );
    assert_eq!(serde_json::from_value::<Change>(value).unwrap(), change);
    let unknown: Brief = serde_json::from_value(json!({"entry": 7})).unwrap();
    assert_eq!(unknown.from, None);
}
