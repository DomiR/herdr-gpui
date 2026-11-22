use super::*;
use crate::push::directives::{without_directives, wizzes};
use sheprd_chat_protocol::{Change, Entry, EntryId, EntryKind, Transcript, TurnEnd, TurnSummary};

#[test]
fn a_wizz_line_asks_for_attention() {
    assert!(wizzes("I need you\n::wizz"));
    assert!(wizzes("  ::wizz  \nCome look"));
    assert!(!wizzes("::confetti\nDone"));
    assert!(!wizzes("say ::wizz in a sentence"));
    assert!(!wizzes(":::wizz\nnot a bubble\n:::"));
}

#[test]
fn a_wizz_in_code_or_indented_as_code_is_not_one() {
    assert!(!wizzes("```\n::wizz\n```"));
    assert!(!wizzes("~~~md\n::wizz\n~~~"));
    assert!(!wizzes("    ::wizz"));
    assert!(wizzes("```\ncode\n```\n::wizz"));
    // A longer fence holds a shorter one, as a Markdown example does.
    assert!(!wizzes("````md\n```\n::wizz\n```\n````"));
    assert!(!wizzes("```\n~~~\n::wizz\n```"));
    assert!(!wizzes("```\n``` not a close\n::wizz\n```"));
    assert!(wizzes("````\nx\n`````\n::wizz"));
}

#[test]
fn directives_leave_the_text_a_notification_shows() {
    assert_eq!(
        without_directives(":::slam\nFound it\n:::\n::confetti\n::wizz"),
        "Found it"
    );
    // In code, and in prose, they stay as written.
    assert_eq!(without_directives("```\n::wizz\n```"), "```\n::wizz\n```");
    assert_eq!(without_directives("a ::wizz b"), "a ::wizz b");
}

const CHAT: Chat<'static> = Chat {
    agent: "Claude",
    pane: "w1-p3",
    title: "plan",
    workspace: "herdr-gpui",
};

fn ended(answer: &str, privacy: Privacy) -> Notification {
    ended_after(
        vec![EntryKind::Agent {
            text: answer.to_owned(),
        }],
        privacy,
    )
}

/// The push for a turn ending after `entries`.
fn ended_after(entries: Vec<EntryKind>, privacy: Privacy) -> Notification {
    let mut before = Transcript::new("Claude");
    before.entries = entries
        .into_iter()
        .zip(1..)
        .map(|(kind, id)| Entry {
            id: EntryId(id),
            kind,
        })
        .collect();
    let turn = Change::Push {
        entry: Entry {
            id: EntryId(100),
            kind: EntryKind::Turn(TurnSummary {
                ended_ms: None,
                duration_ms: 1_000,
                end: TurnEnd::Done,
                tokens: None,
            }),
        },
    };
    decide(&CHAT, &before, &turn, privacy).unwrap()
}

#[test]
fn an_earlier_turns_wizz_does_not_nudge_again() {
    let later = ended_after(
        vec![
            EntryKind::Agent {
                text: "Need you\n::wizz".to_owned(),
            },
            EntryKind::User {
                text: "on it".to_owned(),
                files: 0,
                note: None,
            },
        ],
        Privacy::Excerpt,
    );
    assert!(!later.wizz);
    assert_eq!(later.urgency, Urgency::Normal);
}

#[test]
fn a_turn_ending_in_a_wizz_is_urgent_whatever_the_privacy() {
    for privacy in [Privacy::Excerpt, Privacy::Generic] {
        let wizz = ended("Come look at this\n::wizz", privacy);
        assert!(wizz.wizz);
        assert_eq!(wizz.kind, Kind::TurnEnded);
        assert_eq!(wizz.urgency, Urgency::High);
    }
    let plain = ended("All done", Privacy::Excerpt);
    assert!(!plain.wizz);
    assert_eq!(plain.urgency, Urgency::Normal);
}

#[test]
fn a_finished_turns_excerpt_leaves_out_its_directives() {
    let wizz = ended(":::slam\nCome look\n:::\n::wizz", Privacy::Excerpt);
    assert!(wizz.body.starts_with("Come look\n"), "{}", wizz.body);
}

#[test]
fn a_wizz_is_never_held_back_by_the_throttle() {
    let mut throttle = Throttle::default();
    let plain = ended("one", Privacy::Excerpt);
    assert!(throttle.offer(plain.clone(), 0).is_some());
    assert!(throttle.offer(plain, 1_000).is_none());
    assert!(
        throttle
            .offer(ended("now\n::wizz", Privacy::Excerpt), 2_000)
            .is_some()
    );
}
