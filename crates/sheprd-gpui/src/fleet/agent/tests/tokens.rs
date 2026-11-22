//! A private relay's listen token reaches the agent's service in a
//! private file, never in the script, and a share waits for a relay that
//! lets the agent listen.

use super::*;

const TOKEN: &str = "a-listen-token-0123456789";

pub(super) fn private(url: &str) -> Relay {
    Relay {
        url: url.to_owned(),
        token: Some(ListenToken::parse(TOKEN).unwrap()),
    }
}

#[test]
fn a_private_relays_token_goes_to_a_file_the_service_names() {
    let relays = [
        public("https://fra1.relay.sheprd.org"),
        private("https://own.example.org"),
    ];
    let script = service_script(&relays);
    assert!(!script.contains(TOKEN), "the token is in the script");
    assert_eq!(service_input(&relays), format!("{TOKEN}\n").into_bytes());
    // The second relay's token file follows it, on both systems.
    assert!(script.contains(
        "--relay https://fra1.relay.sheprd.org --relay https://own.example.org --token-file \\\"$state/tokens/relay-1\\\""
    ));
    assert!(script.contains(
        "<string>--relay</string><string>https://own.example.org</string><string>--token-file</string><string>$state/tokens/relay-1</string>"
    ));
    assert!(script.contains("umask 077"));
    assert!(script.contains("printf '%s\\n' \"$token\" > \"$state/tokens/relay-1\""));
    // Stale files go; a public relay gets none.
    assert!(script.contains("rm -f \"$state/tokens/\"relay-*"));
    assert!(!script.contains("relay-0"));
}

#[test]
fn public_relays_need_no_input() {
    assert!(service_input(&[public("https://fra1.relay.sheprd.org")]).is_empty());
}

/// The service script stores the token it reads, readable by its account
/// alone, and names it for the service.
#[cfg(unix)]
#[test]
fn the_service_script_writes_the_token_privately() {
    use std::{io::Write as _, os::unix::fs::PermissionsExt as _};
    let script = service_script(&[private("https://own.example.org")]);
    let start = script.find("mkdir -p \"$state/tokens\"").unwrap();
    let end = script.find("case \"$(uname -s)\" in").unwrap();
    let dir = tempfile::tempdir().unwrap();
    let state = dir.path().join("state");
    let body = format!("state='{}'\n{}", state.display(), &script[start..end]);
    #[allow(clippy::disallowed_methods)] // A local `sh`, in a test.
    let mut child = std::process::Command::new("sh")
        .args(["-c", &body])
        .stdin(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(&service_input(&[private("https://own.example.org")]))
        .unwrap();
    assert!(child.wait().unwrap().success());
    let file = state.join("tokens/relay-0");
    assert_eq!(
        std::fs::read_to_string(&file).unwrap(),
        format!("{TOKEN}\n")
    );
    let mode = std::fs::metadata(&file).unwrap().permissions().mode();
    assert_eq!(mode & 0o077, 0, "{mode:o}");
}

#[test]
fn a_share_waits_for_a_relay_that_lets_the_agent_listen() {
    let mut refused = report(&[ED25519]);
    refused.relays = [(
        "https://own.example.org".to_owned(),
        RelayState::Failed {
            error: "listen token refused".to_owned(),
        },
    )]
    .into_iter()
    .collect();
    assert!(matches!(running(refused.clone()), Err(Error::NotListening)));
    refused.relays.clear();
    assert!(matches!(running(refused), Err(Error::NotListening)));
}
