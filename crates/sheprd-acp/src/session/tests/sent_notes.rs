//! A prompt's entry keeps what was sent to the agent ahead of the person's
//! words, for a client to show when asked, apart from those words.

use super::*;
use crate::context::{Facts, Prompts};
use sheprd_chat_protocol::{Entry, EntryId};

fn started(config: Config) -> Harness {
    let mut harness = harness_with(config);
    harness.session.start();
    harness
}

fn prompt(text: &str) -> Command {
    Command::Prompt {
        origin: None,
        text: text.to_owned(),
        attachments: Vec::new(),
    }
}

/// The text and note of each prompt in the transcript.
fn prompts(harness: &Harness) -> Vec<(String, Option<String>)> {
    harness
        .transcript()
        .entries
        .iter()
        .filter_map(|entry| match &entry.kind {
            EntryKind::User { text, note, .. } => Some((text.clone(), note.clone())),
            _ => None,
        })
        .collect()
}

#[test]
fn a_prompt_keeps_the_note_sent_ahead_of_it() {
    let mut harness = started(Config {
        prompts: Prompts::with_instructions("Answer in French."),
        ..config()
    });
    let id = harness.last_request("initialize")["id"].as_i64().unwrap();
    harness.respond(id, json!({"protocolVersion": 1, "agentCapabilities": {}}));
    let id = harness.last_request("session/new")["id"].as_i64().unwrap();
    harness.respond(id, json!({"sessionId": "s1"}));
    harness.command(0, prompt("bonjour"));
    let request = harness.last_request("session/prompt");
    let note = request["params"]["prompt"][0]["text"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(note.contains("Answer in French."), "{note}");
    harness.respond(
        request["id"].as_i64().unwrap(),
        json!({"stopReason": "end_turn"}),
    );
    // Nothing new to tell: the next prompt went alone.
    harness.command(0, prompt("encore"));
    assert_eq!(
        prompts(&harness),
        [
            ("bonjour".to_owned(), Some(note)),
            ("encore".to_owned(), None),
        ]
    );
}

#[test]
fn a_replayed_note_goes_with_the_words_after_it_only() {
    let mut harness = started(Config {
        load: Some("old".to_owned()),
        ..config()
    });
    let id = harness.last_request("initialize")["id"].as_i64().unwrap();
    harness.respond(
        id,
        json!({"protocolVersion": 1, "agentCapabilities": {"loadSession": true}}),
    );
    let load = harness.last_request("session/load");
    let note = Context::new(
        Prompts::with_instructions("Be brief."),
        Facts::default(),
        Default::default(),
    )
    .before(None)
    .unwrap();
    let updates = [
        json!({"sessionUpdate": "user_message_chunk", "content": {"type": "text", "text": note}}),
        json!({"sessionUpdate": "user_message_chunk", "content": {"type": "text", "text": "first"}}),
        json!({"sessionUpdate": "agent_message_chunk", "content": {"type": "text", "text": "ok"}}),
        json!({"sessionUpdate": "user_message_chunk", "content": {"type": "text", "text": "second"}}),
    ];
    for update in updates {
        harness.input(Input::Agent(
            HarnessId::default(),
            Inbound::Notification {
                method: "session/update".to_owned(),
                params: json!({"sessionId": "old", "update": update}),
            },
        ));
    }
    harness.respond(load["id"].as_i64().unwrap(), json!({}));
    assert_eq!(
        prompts(&harness),
        [
            ("first".to_owned(), Some(note)),
            ("second".to_owned(), None),
        ]
    );
}

fn noted(id: u64, size: usize) -> Entry {
    Entry {
        id: EntryId(id),
        kind: EntryKind::User {
            text: "hi".to_owned(),
            files: 0,
            note: Some("n".repeat(size)),
        },
    }
}

#[test]
fn a_note_counts_toward_both_byte_budgets() {
    let entry = noted(0, 1000);
    assert_eq!(entry_size(&entry), 1002);
    assert_eq!(
        change_size(&Change::Push {
            entry: entry.clone()
        }),
        1002
    );
    // Notes alone, the words being short, still push old entries out.
    let entries: Vec<_> = (0..40).map(|id| noted(id, 64 * 1024)).collect();
    let keep = keep_from(&entries);
    assert!(keep > 0);
    let kept: usize = entries[keep..].iter().map(entry_size).sum();
    assert!(kept <= MAX_TRANSCRIPT_TEXT, "{kept}");
}

#[test]
fn a_replayed_note_stays_with_a_paste_split_across_chunks() {
    let mut harness = started(Config {
        load: Some("old".to_owned()),
        ..config()
    });
    let id = harness.last_request("initialize")["id"].as_i64().unwrap();
    harness.respond(
        id,
        json!({"protocolVersion": 1, "agentCapabilities": {"loadSession": true}}),
    );
    let load = harness.last_request("session/load");
    let note = Context::new(
        Prompts::with_instructions("Be brief."),
        Facts::default(),
        Default::default(),
    )
    .before(None)
    .unwrap();
    for text in [
        note.as_str(),
        "<pasted_content id=\"0e15\">",
        "\nTask: ship it\n</pasted_",
        "content>",
    ] {
        harness.input(Input::Agent(
            HarnessId::default(),
            Inbound::Notification {
                method: "session/update".to_owned(),
                params: json!({"sessionId": "old", "update": {"sessionUpdate": "user_message_chunk",
                    "content": {"type": "text", "text": text}}}),
            },
        ));
    }
    harness.respond(load["id"].as_i64().unwrap(), json!({}));
    assert_eq!(
        prompts(&harness),
        [("Task: ship it".to_owned(), Some(note))]
    );
}
