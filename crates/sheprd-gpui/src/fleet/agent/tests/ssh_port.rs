//! The agent forwards to the port the host's SSH server listens on, as
//! the sharing session found it, not always 22.

use super::*;

/// The port [`SSH_PORT`] picks when the session says `connection`.
fn port(connection: Option<&str>) -> String {
    let script = format!("{SSH_PORT}printf %s \"$port\"");
    #[allow(clippy::disallowed_methods)] // A local `sh`, in a test.
    let mut command = std::process::Command::new("sh");
    command
        .args(["-c", &script])
        .env_clear()
        .env("PATH", "/usr/bin:/bin");
    if let Some(connection) = connection {
        command.env("SSH_CONNECTION", connection);
    }
    let output = command.output().unwrap();
    assert!(output.status.success());
    String::from_utf8(output.stdout).unwrap()
}

#[test]
fn the_port_the_ssh_server_took_the_session_on_is_forwarded_to() {
    assert_eq!(port(Some("192.0.2.9 51234 192.0.2.1 2222")), "2222");
    assert_eq!(port(Some("::1 51234 ::1 22")), "22");
    // Nothing usable said: the default.
    for said in [
        None,
        Some(""),
        Some("192.0.2.9 51234 192.0.2.1 ssh"),
        Some("a b c 99999"),
    ] {
        assert_eq!(port(said), "22", "{said:?}");
    }
}

#[test]
fn both_services_forward_to_that_port() {
    let script = service_script(&[public("https://fra1.relay.sheprd.org")]);
    assert!(script.contains(SSH_PORT));
    assert!(script.contains("run --state-dir \\\"$state\\\" --forward $port --relay"));
    assert!(script.contains("<string>--forward</string><string>$port</string>"));
}
