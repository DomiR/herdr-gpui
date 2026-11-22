//! The brief a chat starts with, and briefs marked again on a continued
//! session.

use super::*;
use sheprd_chat_protocol::{Brief, Entry, EntryId};

const BRIEF: &str = "Task: make the iOS app use the hosted push relay.";

/// The session's start up to the agent's `initialize` answer, before any
/// session opens.
fn initialized(harness: &mut Harness) {
    harness.session.start();
    let initialize = harness.last_request("initialize");
    harness.respond(
        initialize["id"].as_i64().unwrap(),
        json!({"protocolVersion": 1, "agentCapabilities": {}}),
    );
}

fn open(harness: &mut Harness, method: &str) {
    let request = harness.last_request(method);
    harness.respond(request["id"].as_i64().unwrap(), json!({"sessionId": "s1"}));
}

#[test]
fn the_brief_waits_for_the_session_then_goes_first_and_is_marked() {
    let dir = tempfile::tempdir().unwrap();
    let mut harness = harness_with(config());
    harness.session.keep_briefs(dir.path().to_owned());
    initialized(&mut harness);
    harness.session.brief(crate::briefs::Brief {
        from: Some(Agent::Codex),
        text: BRIEF.to_owned(),
    });
    // Queued while the session opens, like a client's prompt.
    assert_eq!(harness.transcript().queued.len(), 1);
    open(&mut harness, "session/new");
    let request = harness.last_request("session/prompt");
    assert!(
        request["params"]["prompt"]
            .to_string()
            .contains("hosted push relay")
    );
    let transcript = harness.transcript();
    let entry = transcript
        .entries
        .iter()
        .find(|entry| matches!(entry.kind, EntryKind::User { .. }))
        .unwrap();
    assert_eq!(entry.kind.text(), Some(BRIEF));
    assert_eq!(
        transcript.briefs,
        [Brief {
            entry: entry.id,
            from: Some(Agent::Codex),
        }]
    );
    // Kept for the agent's session, so a continued chat marks it again.
    let kept = crate::briefs::load(dir.path(), "s1");
    assert_eq!(kept, [crate::briefs::Known::of(BRIEF, Some(Agent::Codex))]);
}

#[test]
fn a_prompt_the_person_types_is_not_a_brief() {
    let mut harness = ready();
    prompt(&mut harness, 0, "fix the bug");
    assert!(harness.transcript().briefs.is_empty());
}

#[test]
fn a_continued_session_marks_the_brief_its_log_replays() {
    let dir = tempfile::tempdir().unwrap();
    crate::briefs::save(
        dir.path(),
        "s1",
        crate::briefs::Known::of(BRIEF, Some(Agent::Claude)),
    )
    .unwrap();
    let mut history = Transcript::new("Claude");
    history.entries = vec![
        Entry {
            id: EntryId(1),
            // As Claude's log holds it: in the envelope pasted text gets.
            kind: EntryKind::User {
                text: format!("<pasted_content id=\"0e15\">\n{BRIEF}\n</pasted_content>"),
                files: 0,
                note: None,
            },
        },
        Entry {
            id: EntryId(2),
            kind: EntryKind::User {
                text: "carry on".to_owned(),
                files: 0,
                note: None,
            },
        },
    ];
    let mut harness = harness_with(Config {
        history: Some(history),
        load: Some("s1".to_owned()),
        ..config()
    });
    harness.session.keep_briefs(dir.path().to_owned());
    let transcript = harness.transcript();
    // Shown without the envelope.
    assert_eq!(transcript.entries[0].kind.text(), Some(BRIEF));
    assert_eq!(
        transcript.briefs,
        [Brief {
            entry: EntryId(1),
            from: Some(Agent::Claude),
        }]
    );
}

#[test]
fn a_replayed_prompt_matching_a_kept_brief_is_marked_as_it_arrives() {
    let dir = tempfile::tempdir().unwrap();
    crate::briefs::save(dir.path(), "s1", crate::briefs::Known::of(BRIEF, None)).unwrap();
    let mut harness = harness_with(Config {
        load: Some("s1".to_owned()),
        ..config()
    });
    harness.session.keep_briefs(dir.path().to_owned());
    harness.session.start();
    let id = harness.last_request("initialize")["id"].as_i64().unwrap();
    harness.respond(
        id,
        json!({"protocolVersion": 1, "agentCapabilities": {"loadSession": true}}),
    );
    let load = harness.last_request("session/load");
    // An ACP agent replays its history as updates before the load answers.
    harness.input(Input::Agent(
        HarnessId::default(),
        Inbound::Notification {
            method: "session/update".to_owned(),
            params: json!({"sessionId": "s1", "update": {"sessionUpdate": "user_message_chunk",
            "content": {"type": "text", "text": BRIEF}}}),
        },
    ));
    harness.respond(load["id"].as_i64().unwrap(), json!({}));
    let transcript = harness.transcript();
    let entry = transcript
        .entries
        .iter()
        .find(|entry| entry.kind.text() == Some(BRIEF))
        .unwrap();
    assert_eq!(
        transcript.briefs,
        [Brief {
            entry: entry.id,
            from: None
        }]
    );
}
