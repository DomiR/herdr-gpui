//! A fleet session ties a tunnel key to a member only when the device in
//! it proves the member's fleet key: a member that names another device's
//! public tunnel key never speaks for it, nor takes its pairing with it.

use super::*;
use crate::control::fleet::HostId;
use crate::control::fleet::removal::{PROOF_LEN, PURPOSE_SSH};
use crate::desktop::{keys::Keys, record::Record};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use std::path::Path;

/// Pairs the desktop whose keys are in `dir` with `on` one by one, as
/// over Bluetooth.
pub(super) fn pair(on: &Node, dir: &Path) -> crate::control::DeviceId {
    let keys = Keys::load_or_create(dir).unwrap();
    let ssh = crate::desktop::keys::ssh_public_key(dir).unwrap();
    let point = SigningKey::from_slice(&[9; 32])
        .unwrap()
        .verifying_key()
        .to_sec1_point(false);
    let id = crate::control::DeviceId("desktop".into());
    on.state
        .auth()
        .devices_mut()
        .add(
            crate::auth::NewDevice {
                app: None,
                fleet: None,
                id: id.clone(),
                name: "a desktop".into(),
                public_key: crate::auth::PublicKey::parse(
                    &URL_SAFE_NO_PAD.encode(point.as_bytes()),
                )
                .unwrap(),
                relay: Some(crate::control::host_relay::Keys {
                    dial: URL_SAFE_NO_PAD
                        .encode(keys.dial.verifying_key().to_sec1_point(true).as_bytes()),
                    tunnel: URL_SAFE_NO_PAD.encode(keys.tunnel.identity().x25519()),
                }),
                ssh: Some(crate::ssh_access::SshKey::parse(&ssh).unwrap()),
            },
            now_ms(),
        )
        .unwrap();
    id
}

/// A tunnel from the device whose keys are in `dir` to `to`.
async fn dial(dir: &Path, to: &Node) -> sheprd_tunnel::stream::Tunnel {
    let info = to.relays.info().unwrap();
    let record = Record {
        computer: String::new(),
        identity: info.identity,
        relays: info.relays,
        route: info.route,
        fleet: false,
        host: None,
        opening: false,
    };
    let keys = Keys::load_or_create(dir).unwrap();
    for _ in 0..100 {
        if let Ok((connection, tunnel)) = crate::desktop::connect::open_record(&record, &keys).await
        {
            std::mem::forget(connection);
            return tunnel;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    panic!("never let through");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_member_naming_a_desktops_tunnel_key_does_not_take_it() {
    let (http, _, _) = relay().await;
    let (mut a, b) = (Node::new(http, 1).await, Node::new(http, 2).await);
    // A desktop paired with a one by one, in no fleet.
    let desktop = tempfile::tempdir().unwrap();
    let paired = pair(&a, desktop.path());
    assert_eq!(a.ask(Ask::Found), Answer::Done);
    join(&a, &b);
    // b rotates to a bundle naming the desktop's public tunnel key.
    let mut bundle = a.fleet.inner().log.as_ref().unwrap().state().members[&b.fleet.me()]
        .bundle
        .clone();
    bundle.tunnel = Keys::load_or_create(desktop.path())
        .unwrap()
        .tunnel
        .identity()
        .x25519();
    b.fleet.receive(a.ops(), now_ms()).unwrap();
    b.fleet.sign(Body::Rotate(bundle), now_ms()).unwrap();
    a.fleet.receive(b.ops(), now_ms()).unwrap();
    a.start().await;
    // The desktop dials as before: its own pairing lets it in, and the
    // session ties it to no member.
    let mut tunnel = dial(desktop.path(), &a).await;
    tunnel.io.write_all(b"SSH-2.0-desktop\r\n").await.unwrap();
    let mut greeting = [0; 14];
    tunnel.io.read_exact(&mut greeting).await.unwrap();
    assert_eq!(&greeting, b"SSH-2.0-test\r\n");
    assert!(!a.fleet.inner().proven.contains_key(&b.fleet.me()));
    // Removing b leaves the desktop's pairing alone.
    a.ask(Ask::Remove {
        member: b.me(),
        also_members: Vec::new(),
        also_hosts: Vec::new(),
        reason: crate::control::fleet::Reason::Compromised,
    });
    grants::apply(&a.state, &a.fleet).unwrap();
    assert!(a.state.auth().devices().get(&paired).is_some());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_session_without_the_members_proof_ends() {
    let (http, _, _) = relay().await;
    let (mut a, b) = (Node::new(http, 1).await, Node::new(http, 2).await);
    assert_eq!(a.ask(Ask::Found), Answer::Done);
    join(&a, &b);
    a.start().await;
    // b's own tunnel key, but a proof signed with another key.
    let mut tunnel = dial(b.dir.path(), &a).await;
    let mut opening = vec![PURPOSE_SSH];
    opening.extend_from_slice(&crate::control::fleet::removal::prove(
        &SigningKey::from_slice(&[0x42; 32]).unwrap(),
        &tunnel.hash,
    ));
    assert_eq!(opening.len(), 1 + PROOF_LEN);
    tunnel.io.write_all(&opening).await.unwrap();
    let mut byte = [0];
    assert!(matches!(tunnel.io.read(&mut byte).await, Ok(0) | Err(_)));
    assert!(!a.fleet.inner().proven.contains_key(&b.fleet.me()));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_key_an_earlier_build_recorded_does_not_count_as_the_members() {
    let (http, _, _) = relay().await;
    let (mut a, b) = (Node::new(http, 1).await, Node::new(http, 2).await);
    // b, also paired with a one by one, and allowed on a by the fleet.
    let paired = pair(&a, b.dir.path());
    assert_eq!(a.ask(Ask::Found), Answer::Done);
    join(&a, &b);
    a.fleet.receive(b.ops(), now_ms()).unwrap();
    // An earlier build recorded b's tunnel key on a handshake alone.
    let tunnel = Keys::load_or_create(b.dir.path())
        .unwrap()
        .tunnel
        .identity()
        .x25519();
    std::fs::write(
        a.dir.path().join("fleet/peers.json"),
        format!(
            "{{\"proven\": {{\"{}\": [\"{}\"]}}}}",
            b.me(),
            HostId(tunnel)
        ),
    )
    .unwrap();
    let fleet = Arc::new(Fleet::load(a.dir.path(), now_ms()).unwrap());
    assert!(fleet.inner().proven.is_empty());
    Arc::get_mut(&mut a.state).unwrap().fleet = Some(Arc::clone(&fleet));
    a.fleet = fleet;
    a.start().await;
    // With no purpose byte, the session goes through b's own pairing,
    // which stays.
    let mut tunnel = dial(b.dir.path(), &a).await;
    tunnel.io.write_all(b"SSH-2.0-desktop\r\n").await.unwrap();
    let mut greeting = [0; 14];
    tunnel.io.read_exact(&mut greeting).await.unwrap();
    assert_eq!(&greeting, b"SSH-2.0-test\r\n");
    assert!(a.state.auth().devices().get(&paired).is_some());
    assert!(a.fleet.inner().proven.is_empty());
}
