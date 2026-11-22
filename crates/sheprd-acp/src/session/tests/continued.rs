//! The notice under the conversation a continued session brought, which
//! clients fold the earlier turns above.

use super::{
    harnesses::{launch, requests, set_agent},
    *,
};
use sheprd_chat_protocol::{Entry, EntryId, EntryKind, Transcript};

fn history() -> Transcript {
    let mut transcript = Transcript::new("Claude".to_owned());
    transcript.entries = vec![
        Entry {
            id: EntryId(4096),
            kind: EntryKind::User {
                files: 0,
                note: None,
                text: "Fix the build".to_owned(),
            },
        },
        Entry {
            id: EntryId(8192),
            kind: EntryKind::Agent {
                text: "Fixed.".to_owned(),
            },
        },
    ];
    transcript
}

fn open(config: Config, method: &str, capabilities: Value, response: Value) -> Harness {
    let mut harness = harness_with(config);
    harness.session.start();
    let id = harness.last_request("initialize")["id"].as_i64().unwrap();
    harness.respond(
        id,
        json!({"protocolVersion": 1, "agentCapabilities": capabilities}),
    );
    let request = harness.last_request(method);
    harness.respond(request["id"].as_i64().unwrap(), response);
    harness
}

fn resumed(history: Option<Transcript>) -> Harness {
    open(
        Config {
            choices: Some(vec![Agent::Claude, Agent::Codex]),
            history,
            load: Some("old".to_owned()),
            require_load: true,
            ..config()
        },
        "session/load",
        json!({"loadSession": true}),
        json!({}),
    )
}

#[test]
fn a_resumed_conversation_ends_in_a_notice_the_session_names() {
    let harness = resumed(Some(history()));
    let transcript = harness.transcript();
    let last = transcript.entries.last().unwrap();
    assert!(matches!(
        &last.kind,
        EntryKind::Notice { level: NoticeLevel::Info, details: None, text, .. }
            if text.starts_with("Continued from")
    ));
    assert_eq!(
        transcript.session.as_ref().unwrap().continued,
        Some(last.id)
    );
    assert!(last.id > EntryId(8192));
}

#[test]
fn a_resumed_session_without_earlier_words_has_no_divider() {
    let harness = resumed(None);
    assert!(harness.transcript().entries.is_empty());
    assert_eq!(
        harness.transcript().session.as_ref().unwrap().continued,
        None
    );
}

#[test]
fn a_new_session_has_no_divider() {
    let harness = open(
        config(),
        "session/new",
        json!({}),
        json!({"sessionId": "fresh"}),
    );
    assert_eq!(
        harness.transcript().session.as_ref().unwrap().continued,
        None
    );
    assert!(harness.transcript().entries.is_empty());
}

/// Answers the agent started second, by its request `method`.
fn answer_second(harness: &mut Harness, agent: &Receiver<Vec<u8>>, method: &str, result: Value) {
    let request = requests(agent)
        .into_iter()
        .rfind(|request| request["method"] == method)
        .unwrap();
    harness.input(Input::Agent(
        HarnessId(1),
        Inbound::Response {
            id: wire::RequestId::Number(request["id"].as_i64().unwrap()),
            result: Ok(result),
        },
    ));
}

#[test]
fn a_switch_of_agent_keeps_the_first_divider_and_places_no_other() {
    let mut harness = resumed(Some(history()));
    let divider = harness.transcript().session.as_ref().unwrap().continued;
    assert!(divider.is_some());
    let (flow, _) = set_agent(&mut harness, Agent::Codex);
    assert!(matches!(flow, Flow::Launch { .. }), "{flow:?}");
    let agent = launch(&mut harness);
    answer_second(
        &mut harness,
        &agent,
        "initialize",
        json!({"protocolVersion": 1, "agentCapabilities": {}}),
    );
    answer_second(
        &mut harness,
        &agent,
        "session/new",
        json!({"sessionId": "s2"}),
    );
    let transcript = harness.transcript();
    assert_eq!(transcript.session.as_ref().unwrap().id, "s2");
    assert_eq!(transcript.session.as_ref().unwrap().continued, divider);
    let dividers = transcript
        .entries
        .iter()
        .filter(|entry| {
            matches!(&entry.kind, EntryKind::Notice { text, .. } if text.starts_with("Continued from"))
        })
        .count();
    assert_eq!(dividers, 1);
}
