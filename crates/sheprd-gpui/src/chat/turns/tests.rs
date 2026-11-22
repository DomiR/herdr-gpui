use super::*;

fn user(id: u64, text: &str) -> Entry {
    Entry {
        id: EntryId(id),
        kind: EntryKind::User {
            text: text.into(),
            files: 0,
            note: None,
        },
    }
}

fn agent(id: u64, text: &str) -> Entry {
    Entry {
        id: EntryId(id),
        kind: EntryKind::Agent { text: text.into() },
    }
}

fn entries() -> Vec<Entry> {
    vec![
        agent(0, "Hello."),
        user(1, "first\nsecond line"),
        agent(2, "The   first\nreply."),
        agent(3, "More."),
        user(4, "again"),
    ]
}

#[test]
fn each_prompt_is_a_turn_with_the_start_of_its_reply() {
    let turns = turns(&entries());
    assert_eq!(turns.len(), 2);
    assert_eq!(turns[0].entry, EntryId(1));
    assert_eq!(turns[0].index, 1);
    assert_eq!(turns[0].label, "first second line");
    assert_eq!(turns[0].summary, "The first reply.");
    // No reply yet.
    assert_eq!(turns[1].index, 4);
    assert_eq!(turns[1].summary, "");
}

#[test]
fn a_long_prompt_is_cut_and_its_attachments_left_out() {
    let long = "word ".repeat(400);
    let files = Entry {
        id: EntryId(9),
        kind: EntryKind::User {
            text: format!("look\n{}a.png", sheprd_chat_protocol::ATTACHED),
            files: 1,
            note: None,
        },
    };
    let turns = turns(&[user(1, &long), files]);
    assert!(
        turns[0].label.chars().count() <= MAX_LABEL,
        "{}",
        turns[0].label
    );
    assert_eq!(turns[1].label, "look");
}

#[test]
fn wide_characters_stay_under_the_timeline_s_limit() {
    let turns = turns(&[user(1, "q"), agent(2, &"\u{1F600}".repeat(400))]);
    assert!(
        turns[0].summary.len() <= MAX_BYTES,
        "{}",
        turns[0].summary.len()
    );
    assert!(!turns[0].summary.is_empty());
}

#[test]
fn a_prompt_of_attachments_alone_is_named_by_them() {
    let files = Entry {
        id: EntryId(1),
        kind: EntryKind::User {
            text: format!("{}a.png", sheprd_chat_protocol::ATTACHED),
            files: 1,
            note: None,
        },
    };
    assert!(turns(&[files])[0].label.ends_with("a.png"));
}

#[test]
fn the_turn_read_is_the_last_prompt_at_or_above_the_top() {
    let turns = turns(&entries());
    assert_eq!(reading(&turns, 0, false).unwrap().index, 1);
    assert_eq!(reading(&turns, 3, false).unwrap().index, 1);
    assert_eq!(reading(&turns, 4, false).unwrap().index, 4);
    // Following the end: the latest, wherever the top is.
    assert_eq!(reading(&turns, 0, true).unwrap().index, 4);
    assert!(reading(&[], 0, false).is_none());
}

#[test]
fn the_ticks_stack_tightly_in_the_gutter_s_middle() {
    // Ten ticks: 10 x 14 + 40 points, centered in 600.
    assert_eq!(stack(10, 600.), (210., 180.));
    // Three would leave no room for a card: as tall as one needs.
    assert_eq!(stack(3, 600.), (252., 96.));
    // More than fit: the whole gutter, from its top.
    assert_eq!(stack(100, 600.), (0., 600.));
}

#[test]
fn the_gutter_is_what_the_column_leaves_on_its_left() {
    assert_eq!(gutter(1200., 800), 200.);
    assert_eq!(gutter(600., 800), 0.);
    // No limit: the column takes it all.
    assert_eq!(gutter(1200., 0), 0.);
}
