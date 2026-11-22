#![allow(clippy::unwrap_used)]
use super::*;
use core::prelude::v1::test;

#[test]
fn requests_are_numbered_apart() {
    let first = next_request();
    let second = next_request();
    assert!(second > first);
}

#[test]
fn dropping_a_request_stops_its_work() {
    let cancelled = Arc::new(AtomicBool::new(false));
    drop(Running {
        way: Way::Claude,
        request: 1,
        cancelled: cancelled.clone(),
    });
    assert!(cancelled.load(Ordering::Acquire));
}

#[test]
fn claude_reads_the_end_of_each_source_and_no_more_than_fits() {
    let long = format!("start {} end", "x".repeat(10_000));
    let kept = work::tail(&long);
    assert!(kept.ends_with("x end"));
    assert!(!kept.contains("start"));
    assert_eq!(work::tail("short"), "short");
    let input = work::bounded(vec!["a".repeat(7_000), "b".repeat(7_000), "c".into()]);
    assert!(input.chars().count() <= sheprd_chat_protocol::excerpt::MAX_INPUT_CHARS);
    assert!(input.starts_with('a'));
    assert!(
        !input.contains('b'),
        "a part that does not fit ends the input"
    );
    assert_eq!(work::bounded(vec!["a".into(), "b".into()]), "a\n\nb");
}

#[test]
fn a_known_terminal_agent_is_called_as_its_chats_are() {
    assert_eq!(sources::display_name("claude"), "Claude");
    assert_eq!(sources::display_name("opencode"), "OpenCode");
    assert_eq!(sources::display_name("Claude"), "Claude");
    assert_eq!(sources::display_name("droid"), "droid");
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn terminal(agent: Option<&str>, role: work::Role) -> work::Terminal {
    work::Terminal {
        agent: agent.map(str::to_owned),
        reach: work::Reach::Herdr {
            host: crate::pane_move::host_for(&sheprd_client::ConnectTarget::Local).unwrap(),
            pane: "w1:p1".into(),
        },
        role,
    }
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn an_observed_chats_agent_is_asked_in_its_terminal() {
    // A chat observing a terminal agent is read-only: its terminal is asked.
    let sources = sources::Sources::for_test(
        &[("Claude", false)],
        vec![terminal(Some("Claude"), work::Role::Ask)],
    );
    assert_eq!(sources.asked_agent(), Some("Claude"));
    assert!(matches!(sources.into_asked(), Some(Asked::Terminal(_))));
    let chat = sources::Sources::for_test(
        &[("Codex", true)],
        vec![terminal(Some("Claude"), work::Role::ReadAndAsk)],
    );
    assert_eq!(chat.asked_agent(), Some("Codex"));
    assert!(matches!(chat.into_asked(), Some(Asked::Chat { .. })));
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn a_mirrors_agent_is_never_asked() {
    let mirror = sources::Sources::for_test(
        &[("Claude", false)],
        vec![terminal(Some("Claude"), work::Role::Read)],
    );
    assert_eq!(mirror.asked_agent(), None);
    assert!(mirror.into_asked().is_none());
}

#[test]
fn roles_say_what_naming_may_do() {
    use work::Role;
    assert!(Role::ReadAndAsk.reads() && Role::ReadAndAsk.asks());
    assert!(Role::Read.reads() && !Role::Read.asks());
    assert!(!Role::Ask.reads() && Role::Ask.asks());
}
