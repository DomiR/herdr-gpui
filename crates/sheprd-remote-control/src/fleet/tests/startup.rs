//! A second daemon on one state directory stops without touching what the
//! running one keeps; a fleet that does not load fails closed.

use super::*;
use crate::ssh_access::SshKey;

fn line() -> Vec<(String, SshKey)> {
    let key = SshKey::parse(&format!(
        "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAI{}",
        "B".repeat(43)
    ))
    .unwrap();
    vec![("fleet:member".to_owned(), key)]
}

#[tokio::test(flavor = "multi_thread")]
async fn a_second_daemon_leaves_the_first_ones_lines() {
    let dir = tempfile::tempdir().unwrap();
    let keys = AuthorizedKeys::at(dir.path().join("authorized_keys"));
    let first = Fleet::for_daemon(dir.path(), now_ms(), Some(&keys))
        .unwrap()
        .unwrap();
    keys.set_fleet(&line()).unwrap();
    let before = std::fs::read(dir.path().join("authorized_keys")).unwrap();
    assert!(matches!(
        Fleet::for_daemon(dir.path(), now_ms(), Some(&keys)),
        Err(Error::Busy)
    ));
    assert_eq!(
        std::fs::read(dir.path().join("authorized_keys")).unwrap(),
        before
    );
    drop(first);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_fleet_that_does_not_load_takes_its_lines_away() {
    let dir = tempfile::tempdir().unwrap();
    let keys = AuthorizedKeys::at(dir.path().join("authorized_keys"));
    keys.set_fleet(&line()).unwrap();
    std::fs::create_dir_all(dir.path().join("fleet")).unwrap();
    std::fs::write(dir.path().join("fleet/peers.json"), b"not json").unwrap();
    assert!(
        Fleet::for_daemon(dir.path(), now_ms(), Some(&keys))
            .unwrap()
            .is_none()
    );
    let kept = std::fs::read_to_string(dir.path().join("authorized_keys")).unwrap();
    assert!(!kept.contains("sheprd-fleet:"));
}
