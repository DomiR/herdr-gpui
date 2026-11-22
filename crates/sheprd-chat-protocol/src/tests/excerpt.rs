use super::*;
use crate::excerpt::{MAX_INPUT_CHARS, one_line, transcript};

fn entry(id: u64, kind: EntryKind) -> Entry {
    Entry {
        id: EntryId(id),
        kind,
    }
}

fn user(id: u64, text: &str) -> Entry {
    entry(
        id,
        EntryKind::User {
            text: text.into(),
            files: 0,
            note: None,
        },
    )
}

fn agent(id: u64, text: &str) -> Entry {
    entry(id, EntryKind::Agent { text: text.into() })
}

#[test]
fn one_line_joins_whitespace_and_cuts_with_an_ellipsis() {
    assert_eq!(one_line("a\n\n b\tc", 10), "a b c");
    assert_eq!(one_line("abcdef", 4), "abc…");
}

#[test]
fn the_excerpt_keeps_what_was_said_oldest_first() {
    let mut chat = Transcript::new("Claude");
    assert!(transcript(&chat).is_empty());
    chat.entries = vec![
        user(1, "Fix the\nlogin"),
        entry(
            2,
            EntryKind::Thought {
                text: "hidden".into(),
            },
        ),
        agent(3, "Fixed it."),
    ];
    assert_eq!(
        transcript(&chat),
        "Person: Fix the login\n\nAgent: Fixed it."
    );
}

#[test]
fn the_excerpt_keeps_the_end_of_a_long_chat() {
    let mut chat = Transcript::new("Claude");
    chat.entries = (1..=40)
        .map(|id| agent(id, &format!("{id} {}", "x".repeat(1_000))))
        .collect();
    let input = transcript(&chat);
    assert!(input.chars().count() <= MAX_INPUT_CHARS);
    assert!(input.ends_with(&"x".repeat(100)));
    assert!(input.contains("Agent: 40 "));
    assert!(!input.contains("Agent: 1 "));
}
