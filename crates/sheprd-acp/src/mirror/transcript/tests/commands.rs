//! A slash command or `!` line the person ran in Claude's terminal, which
//! Claude records in its own markup.

use super::*;

fn user(uuid: &str, text: &str) -> Value {
    json!({"type":"user","uuid":uuid,"message":{"role":"user","content":text}})
}

fn notice_text(entry: &Entry) -> Option<&str> {
    match &entry.kind {
        EntryKind::Notice { text, .. } => Some(text),
        _ => None,
    }
}

#[test]
fn a_slash_command_and_its_output_are_notices() {
    let bytes = lines(&[
        user(
            "c1",
            "<command-name>/model</command-name>\n            <command-message>model</command-message>\n            <command-args>opus</command-args>",
        ),
        user(
            "o1",
            "<local-command-stdout>Set model to Opus\nmore</local-command-stdout>",
        ),
    ]);
    let transcript = parse(TranscriptAgent::Claude, &bytes, 0);
    let texts: Vec<_> = transcript.entries.iter().filter_map(notice_text).collect();
    assert_eq!(
        texts,
        ["Ran /model opus", "Command output · Set model to Opus"]
    );
    assert!(matches!(
        &transcript.entries[1].kind,
        EntryKind::Notice { details: Some(details), .. } if details == "Set model to Opus\nmore"
    ));
}

#[test]
fn empty_command_output_is_not_shown() {
    let bytes = lines(&[
        user(
            "c1",
            "<command-name>/clear</command-name>\n<command-message>clear</command-message>\n<command-args></command-args>",
        ),
        user(
            "o1",
            "<local-command-stdout>(no content)</local-command-stdout>",
        ),
        user(
            "o2",
            "<bash-stdout></bash-stdout><bash-stderr></bash-stderr>",
        ),
    ]);
    let transcript = parse(TranscriptAgent::Claude, &bytes, 0);
    let texts: Vec<_> = transcript.entries.iter().filter_map(notice_text).collect();
    assert_eq!(texts, ["Ran /clear"]);
}

#[test]
fn a_shell_line_and_its_output_are_notices() {
    let bytes = lines(&[
        user("b1", "<bash-input>git status</bash-input>"),
        user(
            "b2",
            "<bash-stdout>On branch main</bash-stdout><bash-stderr>warning: x</bash-stderr>",
        ),
    ]);
    let transcript = parse(TranscriptAgent::Claude, &bytes, 0);
    let texts: Vec<_> = transcript.entries.iter().filter_map(notice_text).collect();
    assert_eq!(texts, ["Ran ! git status", "Shell output · On branch main"]);
}

#[test]
fn words_around_a_tag_stay_the_persons() {
    let text = "Why does <command-name>/x</command-name> show up?";
    let transcript = parse(TranscriptAgent::Claude, &lines(&[user("u1", text)]), 0);
    assert_eq!(
        transcript.entries[0].kind,
        EntryKind::User {
            files: 0,
            note: None,
            text: text.into()
        }
    );
}

#[test]
fn text_beside_an_envelope_keeps_the_message_the_persons() {
    for text in [
        "<local-command-stdout></local-command-stdout>\nExplain this <bash-stdout></bash-stdout>",
        "<local-command-stdout>(no content)</local-command-stdout> and why?",
        "Explain <bash-stdout></bash-stdout>",
        "<bash-input>ls</bash-input>\nthen tell me what you see",
        "<command-name>/x</command-name><command-name>/y</command-name>",
        "<command-args>a</command-args>",
    ] {
        let transcript = parse(TranscriptAgent::Claude, &lines(&[user("u1", text)]), 0);
        assert_eq!(
            transcript.entries[0].kind,
            EntryKind::User {
                files: 0,
                note: None,
                text: text.into()
            },
            "{text}"
        );
    }
}
