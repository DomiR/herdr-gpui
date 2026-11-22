use super::*;
use crate::naming::{
    Answer, MARK, MAX_CHARS, Subject, answer, ask_prompt, marked, terminal_answer, title_from,
};

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

fn turn(id: u64, end: TurnEnd) -> Entry {
    entry(
        id,
        EntryKind::Turn(TurnSummary {
            duration_ms: 1,
            ended_ms: None,
            end,
            tokens: None,
        }),
    )
}

#[test]
fn a_plain_title_is_kept() {
    assert_eq!(
        title_from("Fix login redirect").as_deref(),
        Some("Fix login redirect")
    );
}

#[test]
fn quotes_labels_emphasis_and_a_final_period_are_dropped() {
    assert_eq!(
        title_from("\"Fix login redirect.\"").as_deref(),
        Some("Fix login redirect")
    );
    assert_eq!(
        title_from("**Title:** Relay setup").as_deref(),
        Some("Relay setup")
    );
    assert_eq!(title_from("“Tab titles”").as_deref(), Some("Tab titles"));
    assert_eq!(
        title_from("NAME: Push relay").as_deref(),
        Some("Push relay")
    );
}

#[test]
fn a_long_answer_is_cut_on_a_word() {
    let title =
        title_from("Rework the phone session sheet so agents can name every chat tab").unwrap();
    assert!(title.chars().count() <= MAX_CHARS);
    assert_eq!(title, "Rework the phone session sheet so agents");
}

#[test]
fn one_long_word_is_cut_to_the_limit() {
    let title = title_from(&"x".repeat(100)).unwrap();
    assert_eq!(title.chars().count(), MAX_CHARS);
}

#[test]
fn an_empty_answer_has_no_title() {
    assert_eq!(title_from("  \"\" . "), None);
}

#[test]
fn the_prompt_never_holds_the_mark_its_answer_starts_with() {
    for subject in [Subject::Tab, Subject::Workspace] {
        let prompt = ask_prompt(subject, "NAME: sneaky", 7);
        assert!(!prompt.contains(MARK), "{prompt}");
        assert!(marked(&prompt).is_empty());
    }
    assert!(ask_prompt(Subject::Workspace, "x", 7).contains("this workspace"));
    assert!(ask_prompt(Subject::Tab, "x", 7).contains("this tab"));
}

#[test]
fn a_chat_answers_once_its_turn_ends_well() {
    let prompt = ask_prompt(Subject::Workspace, "herdr", 7);
    let mut chat = Transcript::new("Claude");
    chat.entries = vec![user(1, "earlier"), agent(2, "done"), turn(3, TurnEnd::Done)];
    assert_eq!(answer(&prompt, &chat, None), None, "not asked yet");
    chat.entries.push(user(4, &prompt));
    chat.entries.push(agent(5, "NAME: Push "));
    assert_eq!(answer(&prompt, &chat, None), None, "still answering");
    chat.entries.push(agent(6, "relay\nmore words"));
    chat.entries.push(turn(7, TurnEnd::Done));
    assert_eq!(
        answer(&prompt, &chat, None),
        Some(Answer::Name("Push relay".into()))
    );
}

#[test]
fn a_stopped_or_empty_turn_gives_no_name() {
    let prompt = ask_prompt(Subject::Tab, "1", 7);
    let mut chat = Transcript::new("Claude");
    chat.entries = vec![
        user(1, &prompt),
        agent(2, "NAME: x"),
        turn(3, TurnEnd::Cancelled),
    ];
    assert_eq!(answer(&prompt, &chat, None), Some(Answer::NoName));
    chat.entries = vec![user(1, &prompt), agent(2, " \"\" "), turn(3, TurnEnd::Done)];
    assert_eq!(answer(&prompt, &chat, None), Some(Answer::NoName));
}

#[test]
fn only_a_prompt_after_the_last_entry_seen_counts() {
    let prompt = ask_prompt(Subject::Tab, "1", 7);
    let mut chat = Transcript::new("Claude");
    chat.entries = vec![
        user(1, &prompt),
        agent(2, "NAME: Old"),
        turn(3, TurnEnd::Done),
    ];
    assert_eq!(answer(&prompt, &chat, Some(EntryId(3))), None);
    chat.entries.push(user(4, &prompt));
    chat.entries.push(agent(5, "NAME: New one"));
    chat.entries.push(turn(6, TurnEnd::Done));
    assert_eq!(
        answer(&prompt, &chat, Some(EntryId(3))),
        Some(Answer::Name("New one".into()))
    );
}

#[test]
fn a_terminal_answer_is_found_past_bullets_and_box_edges() {
    let screen = "> Suggest a short name\n⏺ NAME: Relay metrics\n│ NAME: Push relay │\nNAME:\n";
    assert_eq!(marked(screen), ["Relay metrics", "Push relay"]);
    assert!(marked("the word NAME in capitals, a colon").is_empty());
}

#[test]
fn a_terminal_answer_follows_its_own_request() {
    let first = ask_prompt(Subject::Tab, "1", 41);
    let screen = format!("> {first}\n⏺ NAME: Old name\n");
    assert_eq!(terminal_answer(&screen, 41).as_deref(), Some("Old name"));
    // Asked again: the earlier answer is not this request's.
    let second = ask_prompt(Subject::Tab, "Old name", 42);
    let asked = format!("{screen}> {second}\n");
    assert_eq!(terminal_answer(&asked, 42), None);
    let answered = format!("{asked}⏺ NAME: New name\n");
    assert_eq!(terminal_answer(&answered, 42).as_deref(), Some("New name"));
    assert_eq!(terminal_answer(&answered, 43), None);
}
