//! What tells phones the badge moved.

use super::*;

fn panes<const N: usize>(names: [&str; N]) -> BTreeSet<String> {
    names.into_iter().map(str::to_owned).collect()
}

/// What each notification closes, and under which tag; each carries an
/// event id of its own.
fn closes(sent: &[Notification]) -> Vec<(bool, &str, &str)> {
    sent.iter()
        .map(|sent| (sent.clear, sent.pane.as_str(), sent.tag.as_str()))
        .collect()
}

#[test]
fn nothing_goes_out_while_the_count_stands() {
    assert!(badge_changes(&panes(["a"]), &panes(["a"])).is_empty());
}

#[test]
fn a_pane_that_stopped_counting_closes_its_notification() {
    let sent = badge_changes(&panes(["a", "b"]), &panes(["b"]));
    assert_eq!(closes(&sent), [(true, "a", "chat-a")]);
}

#[test]
fn a_count_that_only_grew_closes_nothing_of_the_new_pane() {
    let sent = badge_changes(&panes(["a"]), &panes(["a", "b"]));
    assert_eq!(closes(&sent), [(true, "", "chat-")]);
    assert_ne!(sent[0].tag, Notification::cleared("b").tag);
    assert!(sent[0].pane.is_empty());
}

#[test]
fn a_swap_sends_the_clear_alone_since_it_carries_the_count() {
    let sent = badge_changes(&panes(["a"]), &panes(["b"]));
    assert_eq!(closes(&sent), [(true, "a", "chat-a")]);
}
