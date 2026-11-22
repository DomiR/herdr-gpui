use super::*;

#[test]
fn chat_new_carries_a_brief_only_when_given() {
    let op: ChatOp =
        serde_json::from_str(r#"{"op":"new","agent":"codex","prompt":"Fix the relay"}"#).unwrap();
    assert_eq!(
        op,
        ChatOp::New {
            agent: Some("codex".into()),
            pane: None,
            prompt: Some("Fix the relay".into()),
        }
    );
    let text = serde_json::to_string(&op).unwrap();
    assert_eq!(
        text,
        r#"{"op":"new","agent":"codex","pane":null,"prompt":"Fix the relay"}"#
    );
    // Without one the request reads as before, so an app that predates
    // briefs still takes it.
    let plain = ChatOp::New {
        agent: None,
        pane: None,
        prompt: None,
    };
    assert!(!serde_json::to_string(&plain).unwrap().contains("prompt"));
}

#[test]
fn a_brief_is_neither_blank_nor_past_its_bound() {
    assert!(valid_brief("Fix the relay"));
    assert!(valid_brief(&"a".repeat(MAX_BRIEF)));
    assert!(!valid_brief(&"a".repeat(MAX_BRIEF + 1)));
    assert!(!valid_brief(" \n\t"));
    assert!(!valid_brief(""));
}

#[test]
fn the_longest_brief_fits_a_message_even_escaped() {
    // Newlines, the usual worst case, escape to two bytes each.
    let request = Request::Chat(Call {
        caller: Caller::default(),
        op: ChatOp::New {
            agent: None,
            pane: None,
            prompt: Some("\n".repeat(MAX_BRIEF)),
        },
    });
    let text = serde_json::to_string(&request).unwrap();
    assert!(text.len() < crate::protocol::MAX_MESSAGE, "{}", text.len());
}
