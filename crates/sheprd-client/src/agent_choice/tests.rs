#![allow(clippy::unwrap_used)]
use super::*;
#[cfg(unix)]
use std::time::Instant;

#[test]
fn only_a_program_that_offers_the_choice_is_asked_for_it() {
    let help = sheprd_help("  --choose       Let clients switch to another installed agent\n");
    assert_eq!(AgentChoice::from_help(help.as_bytes()), AgentChoice::Open);
    let older = sheprd_help("  --acp          Use the agent's ACP adapter\n");
    assert_eq!(AgentChoice::from_help(older.as_bytes()), AgentChoice::Fixed);
    assert_eq!(AgentChoice::from_help(b""), AgentChoice::Fixed);
}

fn sheprd_help(options: &str) -> String {
    format!("Usage:\n  sheprd-acp [OPTIONS] AGENT\n\nOptions:\n{options}")
}

#[cfg(unix)]
#[test]
fn a_program_that_cannot_run_offers_no_choice() {
    assert_eq!(
        AgentChoice::of(Path::new("/nonexistent/sheprd-acp")),
        AgentChoice::Fixed
    );
}

/// A wrapper whose child keeps the help's pipe open is given up on at the
/// deadline, rather than leaving the program's lookup unanswered.
#[cfg(unix)]
#[test]
fn a_program_whose_help_never_ends_offers_no_choice() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let wrapper = dir.path().join("sheprd-acp");
    std::fs::write(&wrapper, "#!/bin/sh\necho '  --choose'\nsleep 30 &\nwait\n").unwrap();
    std::fs::set_permissions(&wrapper, std::fs::Permissions::from_mode(0o755)).unwrap();
    let started = Instant::now();
    let deadline = Duration::from_millis(300);
    assert_eq!(
        AgentChoice::of_within(&wrapper, deadline),
        AgentChoice::Fixed
    );
    assert!(started.elapsed() < Duration::from_secs(5));

    let quick = dir.path().join("quick");
    std::fs::write(
        &quick,
        "#!/bin/sh\necho '  --choose   Let clients switch'\n",
    )
    .unwrap();
    std::fs::set_permissions(&quick, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert_eq!(
        AgentChoice::of_within(&quick, Duration::from_secs(5)),
        AgentChoice::Open
    );
}
