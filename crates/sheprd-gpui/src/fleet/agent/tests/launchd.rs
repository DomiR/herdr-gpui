//! On a Mac the agent's job goes in a launchd domain the account has: its
//! graphical session when it is logged in at the screen, else its user
//! domain, so a Mac reached only over SSH runs the agent too.

use super::*;
use std::os::unix::fs::PermissionsExt as _;

/// The domain [`LAUNCHD_DOMAIN`] picks with a stand-in `launchctl` that
/// knows a graphical session or not.
fn domain(graphical: bool) -> String {
    let dir = tempfile::tempdir().unwrap();
    let launchctl = dir.path().join("launchctl");
    let answer = if graphical { "exit 0" } else { "exit 113" };
    std::fs::write(
        &launchctl,
        format!("#!/bin/sh\ncase \"$2\" in gui/*) {answer};; *) exit 0;; esac\n"),
    )
    .unwrap();
    std::fs::set_permissions(&launchctl, std::fs::Permissions::from_mode(0o755)).unwrap();
    let script = format!("{LAUNCHD_DOMAIN}printf %s \"$domain\"");
    #[allow(clippy::disallowed_methods)] // A local `sh`, in a test.
    let output = std::process::Command::new("sh")
        .args(["-c", &script])
        .env_clear()
        .env("PATH", format!("{}:/usr/bin:/bin", dir.path().display()))
        .output()
        .unwrap();
    assert!(output.status.success());
    String::from_utf8(output.stdout).unwrap()
}

#[test]
fn an_ssh_only_mac_gets_its_user_domain_and_a_logged_in_one_its_session() {
    let uid = {
        #[allow(clippy::disallowed_methods)] // A local `id`, in a test.
        let output = std::process::Command::new("id").arg("-u").output().unwrap();
        String::from_utf8(output.stdout).unwrap().trim().to_owned()
    };
    assert_eq!(domain(false), format!("user/{uid}"));
    assert_eq!(domain(true), format!("gui/{uid}"));
}

#[test]
fn the_job_goes_in_the_chosen_domain_and_loads_in_either_session() {
    let script = service_script(&[public("https://fra1.relay.sheprd.org")]);
    assert!(script.contains(LAUNCHD_DOMAIN));
    assert!(script.contains("job=\"$domain/org.sheprd.relay-agent\""));
    assert!(script.contains("launchctl bootstrap \"$domain\" \"$plist\""));
    assert!(!script.contains("bootstrap \"gui/"));
    assert!(script.contains("<string>Aqua</string><string>Background</string>"));
}
