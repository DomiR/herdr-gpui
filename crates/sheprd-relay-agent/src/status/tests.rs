#![allow(clippy::unwrap_used)]

use super::*;

#[test]
fn host_keys_are_read_strongest_first_without_comments() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("ssh_host_rsa_key.pub"),
        "ssh-rsa AAAAB3NzaC1yc2E= root@host\n",
    )
    .unwrap();
    std::fs::write(
        dir.path().join("ssh_host_ed25519_key.pub"),
        "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5 root@host\n",
    )
    .unwrap();
    // A file that holds no key is passed over.
    std::fs::write(dir.path().join("ssh_host_ecdsa_key.pub"), "not a key\n").unwrap();
    assert_eq!(
        host_keys(dir.path()),
        [
            "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5",
            "ssh-rsa AAAAB3NzaC1yc2E="
        ]
    );
    assert!(host_keys(&dir.path().join("missing")).is_empty());
}

#[test]
fn a_host_key_with_odd_characters_is_refused() {
    assert_eq!(host_key("ssh-ed25519 AAAA;rm"), None);
    assert_eq!(host_key("ssh-ed25519\nAAAA"), None);
    assert_eq!(host_key("x-ed25519 AAAA"), None);
}

#[test]
fn a_report_from_an_older_agent_parses_without_host_keys() {
    let old = br#"{"authorized":0,"identity":"id","relays":{},"route":"r","running":false,"version":"0.1.0"}"#;
    let report = Report::parse(old).unwrap();
    assert!(report.ssh_host_keys.is_empty());
    assert!(report.fleet.is_none());
    assert!(matches!(
        Report::parse(b"{"),
        Err(Error::StatusFormat { .. })
    ));
    assert!(matches!(
        Report::parse(&vec![b' '; MAX_REPORT_BYTES + 1]),
        Err(Error::TooLarge { .. })
    ));
}
