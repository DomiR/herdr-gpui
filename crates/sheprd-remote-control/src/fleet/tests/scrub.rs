//! `fleet scrub`: the fleet forgotten with every way it let someone in,
//! answered only once access is gone, and never stopped by a damaged file.

use super::*;
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};

/// `a` founds a fleet `b` joins, with `b`'s grants written: one SSH line
/// for `a` and one relay record for `a`'s host. Returns `b`'s records
/// folder.
async fn joined() -> (Node, Node, PathBuf) {
    let (http, _, _) = relay().await;
    let (a, b) = (Node::new(http, 1).await, Node::new(http, 2).await);
    assert_eq!(a.ask(Ask::Found), Answer::Done);
    join(&a, &b);
    grants::apply(&b.state, &b.fleet).unwrap();
    assert_eq!(b.lines().len(), 1);
    let records = b.dir.path().join("desktop/hosts");
    assert_eq!(records.read_dir().unwrap().count(), 1);
    (a, b, records)
}

/// A desktop paired with `b` one by one, over Bluetooth, whose tunnel key
/// `a` proved.
fn paired_and_proved(a: &Node, b: &Node) -> crate::control::DeviceId {
    let proved = [0x5a; 32];
    let id = crate::control::DeviceId("legacy".into());
    let point = SigningKey::from_slice(&[9; 32])
        .unwrap()
        .verifying_key()
        .to_sec1_point(false);
    b.state
        .auth()
        .devices_mut()
        .add(
            crate::auth::NewDevice {
                app: None,
                fleet: None,
                id: id.clone(),
                name: "a over Bluetooth".into(),
                public_key: crate::auth::PublicKey::parse(
                    &URL_SAFE_NO_PAD.encode(point.as_bytes()),
                )
                .unwrap(),
                relay: Some(crate::control::host_relay::Keys {
                    dial: URL_SAFE_NO_PAD.encode(
                        SigningKey::from_slice(&[8; 32])
                            .unwrap()
                            .verifying_key()
                            .to_sec1_point(true)
                            .as_bytes(),
                    ),
                    tunnel: URL_SAFE_NO_PAD.encode(proved),
                }),
                ssh: Some(
                    crate::ssh_access::SshKey::parse(
                        &crate::desktop::keys::ssh_public_key(a.dir.path()).unwrap(),
                    )
                    .unwrap(),
                ),
            },
            now_ms(),
        )
        .unwrap();
    b.fleet.proved(a.fleet.me(), proved).unwrap();
    id
}

#[tokio::test(flavor = "multi_thread")]
async fn scrubbing_answers_once_access_is_gone() {
    let (a, b, records) = joined().await;
    let legacy = paired_and_proved(&a, &b);
    // Nothing follows the fleet here: the answer itself took access away.
    assert_eq!(b.ask(Ask::Scrub { session: None }), Answer::Done);
    assert!(b.lines().is_empty());
    assert_eq!(records.read_dir().unwrap().count(), 0);
    assert!(b.state.auth().devices().get(&legacy).is_none());
    assert!(ask::view(&b.fleet).is_none());
    // The same key may found or join another fleet after.
    assert!(!b.dir.path().join("fleet/log.bin").exists());
    assert!(!b.dir.path().join("fleet/peers.json").exists());
    assert_eq!(
        Fleet::load(b.dir.path(), now_ms()).unwrap().me(),
        b.fleet.me()
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_damaged_log_does_not_stop_a_scrub() {
    let (a, b, records) = joined().await;
    let legacy = paired_and_proved(&a, &b);
    std::fs::write(b.dir.path().join("fleet/log.bin"), b"damaged").unwrap();
    std::fs::write(b.dir.path().join("fleet/records.json"), b"damaged").unwrap();
    assert!(Fleet::load(b.dir.path(), now_ms()).is_err());
    // As a daemon whose fleet did not load answers it.
    crate::fleet::scrub::unloaded(&b.state).unwrap();
    assert!(b.lines().is_empty());
    assert_eq!(records.read_dir().unwrap().count(), 0);
    assert!(b.state.auth().devices().get(&legacy).is_none());
    assert!(!b.dir.path().join("fleet/log.bin").exists());
    assert!(!b.dir.path().join("fleet/peers.json").exists());
    assert!(Fleet::load(b.dir.path(), now_ms()).is_ok());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_damaged_peers_file_is_kept_and_reported_after_access_goes() {
    let (_, b, records) = joined().await;
    let peers = b.dir.path().join("fleet/peers.json");
    std::fs::write(&peers, b"damaged").unwrap();
    let error = crate::fleet::scrub::unloaded(&b.state).unwrap_err();
    assert!(matches!(error, Error::Peers), "{error:?}");
    assert!(b.lines().is_empty());
    assert_eq!(records.read_dir().unwrap().count(), 0);
    assert!(!b.dir.path().join("fleet/log.bin").exists());
    // Its proven keys could not be revoked: it stays for another try.
    assert_eq!(std::fs::read(&peers).unwrap(), b"damaged");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_scrub_that_cannot_delete_the_log_keeps_the_fleet() {
    use std::os::unix::fs::PermissionsExt as _;
    let (_, b, _) = joined().await;
    let folder = b.dir.path().join("fleet");
    std::fs::set_permissions(&folder, std::fs::Permissions::from_mode(0o500)).unwrap();
    // Root deletes anyway; there is nothing to check then.
    if std::fs::remove_file(folder.join("log.bin")).is_ok() {
        std::fs::set_permissions(&folder, std::fs::Permissions::from_mode(0o700)).unwrap();
        return;
    }
    let error = b.fleet.scrub(None).unwrap_err();
    std::fs::set_permissions(&folder, std::fs::Permissions::from_mode(0o700)).unwrap();
    assert!(
        matches!(
            error,
            Error::Desktop(crate::desktop::Error::File {
                operation: "remove",
                ..
            })
        ),
        "{error:?}"
    );
    // The daemon still holds the fleet its next start reads.
    assert!(ask::view(&b.fleet).is_some());
    assert!(folder.join("log.bin").exists());
}

/// The `sheprd-desktop:` lines in `node`'s `authorized_keys`.
fn desktop_lines(node: &Node) -> usize {
    std::fs::read_to_string(node.dir.path().join("authorized_keys"))
        .unwrap_or_default()
        .lines()
        .filter(|line| line.contains("sheprd-desktop:"))
        .count()
}

#[tokio::test(flavor = "multi_thread")]
async fn a_scrub_with_no_daemon_removes_the_paired_desktops_line() {
    let (a, b, _) = joined().await;
    let legacy = paired_and_proved(&a, &b);
    let keys = AuthorizedKeys::at(b.dir.path().join("authorized_keys"));
    let ssh = crate::ssh_access::SshKey::parse(
        &crate::desktop::keys::ssh_public_key(a.dir.path()).unwrap(),
    )
    .unwrap();
    keys.allow(&legacy, &ssh).unwrap();
    assert_eq!(desktop_lines(&b), 1);
    // As `fleet scrub` with no daemon loads it: no SSH access of its own.
    let mut auth = crate::auth::Auth::load(b.dir.path(), crate::auth::Limits::default()).unwrap();
    grants::revoke_paired(&mut auth, Some(&keys), &[0x5a; 32]).unwrap();
    assert_eq!(desktop_lines(&b), 0);
    assert!(auth.devices().get(&legacy).is_none());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_desktops_line_that_cannot_go_keeps_its_pairing() {
    let (a, b, _) = joined().await;
    let legacy = paired_and_proved(&a, &b);
    let linked = b.dir.path().join("linked_keys");
    std::os::unix::fs::symlink(b.dir.path().join("authorized_keys"), &linked).unwrap();
    let mut auth = crate::auth::Auth::load(b.dir.path(), crate::auth::Limits::default()).unwrap();
    let error = grants::revoke_paired(&mut auth, Some(&AuthorizedKeys::at(linked)), &[0x5a; 32])
        .unwrap_err();
    assert!(matches!(error, Error::SshAccess(_)), "{error:?}");
    // Kept, with the proof that names it, for another try.
    assert!(auth.devices().get(&legacy).is_some());
}

#[test]
fn a_scrub_forgets_the_histories_of_fleets_being_joined() {
    let dir = tempfile::tempdir().unwrap();
    let joining = dir
        .path()
        .join(crate::fleet::store::DIRECTORY)
        .join("joining");
    std::fs::create_dir_all(&joining).unwrap();
    std::fs::write(joining.join(format!("{}.bin", "ab".repeat(32))), b"history").unwrap();
    crate::fleet::store::remove_log(dir.path()).unwrap();
    assert!(!joining.exists());
    // Nothing to remove is no failure.
    crate::fleet::store::remove_log(dir.path()).unwrap();
}
