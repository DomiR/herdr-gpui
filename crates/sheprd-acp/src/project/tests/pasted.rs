//! Prompts show without the envelope agents wrap pasted text in.

use super::*;

const WRAPPED: &str = "<pasted_content id=\"0e15\">\nTask: ship it\n</pasted_content>";

#[test]
fn a_prompt_shows_without_its_paste_envelope() {
    let mut projector = Projector::default();
    let Change::Push { entry } = projector.prompt(WRAPPED, 0, None) else {
        panic!("a prompt is pushed");
    };
    assert_eq!(entry.kind.text(), Some("Task: ship it"));
}

#[test]
fn a_prompt_with_files_keeps_its_text_as_written() {
    let mut projector = Projector::default();
    let text = format!("{WRAPPED}\n{}a.png", sheprd_chat_protocol::ATTACHED);
    let Change::Push { entry } = projector.prompt(&text, 1, None) else {
        panic!("a prompt is pushed");
    };
    assert_eq!(entry.kind.text(), Some(text.as_str()));
}

#[test]
fn a_replayed_prompt_shows_without_its_paste_envelope() {
    let mut projector = Projector::default();
    let transcript = Transcript::new("Claude");
    let change = projector
        .text(&transcript, Stream::User, None, WRAPPED.to_owned())
        .unwrap();
    let Change::Push { entry } = change else {
        panic!("a prompt is pushed");
    };
    assert_eq!(entry.kind.text(), Some("Task: ship it"));
}

/// A replay may split an envelope across chunks: the closing tag, even cut
/// in two, never shows, and the entry ends as the whole message unwrapped.
#[test]
fn a_replayed_prompt_split_across_chunks_shows_without_its_envelope() {
    let mut projector = Projector::default();
    let mut transcript = Transcript::new("Claude");
    for chunk in [
        "<pasted_content id=\"0e15\">",
        "\nTask: ship",
        " it\n</pasted_",
        "content>",
    ] {
        let change = projector
            .text(&transcript, Stream::User, None, chunk.to_owned())
            .unwrap();
        transcript.apply(&change).unwrap();
    }
    assert_eq!(transcript.entries.len(), 1);
    assert_eq!(transcript.entries[0].kind.text(), Some("Task: ship it"));
}

#[test]
fn a_plain_prompt_split_across_chunks_is_appended_as_before() {
    let mut projector = Projector::default();
    let mut transcript = Transcript::new("Claude");
    let first = projector
        .text(&transcript, Stream::User, None, "Fix the ".to_owned())
        .unwrap();
    transcript.apply(&first).unwrap();
    let second = projector
        .text(&transcript, Stream::User, None, "</b> tag".to_owned())
        .unwrap();
    assert!(matches!(second, Change::Append { .. }));
    transcript.apply(&second).unwrap();
    assert_eq!(transcript.entries[0].kind.text(), Some("Fix the </b> tag"));
}
