//! Joining: both directions open, and a log that does not admit this
//! computer is refused.

use super::*;

fn host_files(node: &Node) -> usize {
    std::fs::read_dir(node.dir.path().join("desktop/hosts"))
        .map(|entries| entries.count())
        .unwrap_or(0)
}

#[tokio::test(flavor = "multi_thread")]
async fn every_member_reaches_every_other() {
    let (a, b, c) = three().await;
    for node in [&a, &b, &c] {
        until(10, || node.lines().len() == 2 && host_files(node) == 2).await;
        let view = node.view();
        assert_eq!(view.members.len(), 3);
        assert_eq!(view.hosts.len(), 3);
        // Each line is restricted to loopback, with the fleet's tag.
        for line in node.lines() {
            assert!(
                line.starts_with(
                    "from=\"127.0.0.1,::1\",no-agent-forwarding,no-X11-forwarding ssh-ed25519 "
                ),
                "{line}"
            );
            assert!(
                line.contains(&format!("sheprd-fleet:{}:", &view.fleet[..16])),
                "{line}"
            );
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_log_that_does_not_admit_this_computer_is_refused() {
    let (http, _, _) = relay().await;
    let (a, stranger) = (Node::new(http, 1).await, Node::new(http, 4).await);
    assert_eq!(a.ask(Ask::Found), Answer::Done);
    assert!(matches!(
        stranger.fleet.receive(a.ops(), now_ms()),
        Err(Error::NotAdmitted)
    ));
    assert!(stranger.fleet.inner().log.is_none());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_member_is_in_one_fleet() {
    let (http, _, _) = relay().await;
    let (a, b) = (Node::new(http, 1).await, Node::new(http, 2).await);
    assert_eq!(a.ask(Ask::Found), Answer::Done);
    assert_eq!(b.ask(Ask::Found), Answer::Done);
    let fleet = a.view().fleet;
    assert!(matches!(
        ask::answer(&b.state, &b.fleet, Ask::Bundle { fleet }, now_ms()),
        Err(Error::InFleet)
    ));
    assert!(matches!(
        ask::answer(&a.state, &a.fleet, Ask::Found, now_ms()),
        Err(Error::InFleet)
    ));
}

#[tokio::test(flavor = "multi_thread")]
async fn joining_supersedes_a_pairing_here_so_removal_takes_every_way_in() {
    use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
    let (http, _, _) = relay().await;
    let (mut a, b) = (Node::new(http, 1).await, Node::new(http, 2).await);
    // b was paired with a over Bluetooth before the fleet: a desktop
    // device on a with b's own tunnel and SSH keys.
    let keys = crate::desktop::keys::Keys::load_or_create(b.dir.path()).unwrap();
    let ssh = crate::desktop::keys::ssh_public_key(b.dir.path()).unwrap();
    let phone_key = SigningKey::from_slice(&[9; 32]).unwrap();
    let point = phone_key.verifying_key().to_sec1_point(false);
    let legacy = crate::control::DeviceId("legacy".into());
    a.state
        .auth()
        .devices_mut()
        .add(
            crate::auth::NewDevice {
                app: None,
                fleet: None,
                id: legacy.clone(),
                name: "b over Bluetooth".into(),
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
    assert_eq!(a.ask(Ask::Found), Answer::Done);
    join(&a, &b);
    a.fleet.receive(b.ops(), now_ms()).unwrap();
    a.start().await;
    // A bundle naming b's keys proves nothing: the pairing stays until b
    // itself opens SSH through the fleet with them.
    until(10, || a.lines().len() == 1).await;
    assert!(a.state.auth().devices().get(&legacy).is_some());
    grants::apply(&b.state, &b.fleet).unwrap();
    let info = a.relays.info().unwrap();
    let record = crate::desktop::record::Record {
        computer: String::new(),
        identity: info.identity,
        relays: info.relays,
        route: info.route,
        fleet: true,
        host: None,
        opening: true,
    };
    let host = sheprd_client::relay_host::host_name(&record.id().unwrap());
    let mut opened = None;
    for _ in 0..100 {
        if let Ok(tunnel) = crate::desktop::connect::open(b.dir.path(), &host).await {
            opened = Some(tunnel);
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    let (_connection, mut tunnel) = opened.expect("never let through");
    let mut greeting = [0; 14];
    tunnel.io.read_exact(&mut greeting).await.unwrap();
    // Now the device proved it holds the key, and the fleet lets it in:
    // its pairing goes, and removing it from the fleet removes every way
    // in it had.
    until(10, || a.state.auth().devices().get(&legacy).is_none()).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn removing_a_member_revokes_a_pairing_whose_key_it_proved() {
    use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
    let (http, _, _) = relay().await;
    let (mut a, b) = (Node::new(http, 1).await, Node::new(http, 2).await);
    let keys = crate::desktop::keys::Keys::load_or_create(b.dir.path()).unwrap();
    let ssh = crate::desktop::keys::ssh_public_key(b.dir.path()).unwrap();
    let point = SigningKey::from_slice(&[9; 32])
        .unwrap()
        .verifying_key()
        .to_sec1_point(false);
    let legacy = crate::control::DeviceId("legacy".into());
    a.state
        .auth()
        .devices_mut()
        .add(
            crate::auth::NewDevice {
                app: None,
                fleet: None,
                id: legacy.clone(),
                name: "b over Bluetooth".into(),
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
    assert_eq!(a.ask(Ask::Found), Answer::Done);
    join(&a, &b);
    // a denies b on a: the fleet does not let it in, so its pairing, which
    // the person here made, stays.
    let host = a
        .view()
        .hosts
        .into_iter()
        .find(|host| host.added_by == a.me())
        .unwrap()
        .id;
    a.ask(Ask::Access {
        host,
        member: b.me(),
        allow: false,
    });
    a.start().await;
    let info = a.relays.info().unwrap();
    let record = crate::desktop::record::Record {
        computer: String::new(),
        identity: info.identity,
        relays: info.relays,
        route: info.route,
        fleet: true,
        host: None,
        opening: true,
    };
    // b proves its tunnel key by finishing the handshake, asking for SSH.
    let mut proved = false;
    for _ in 0..100 {
        if let Ok((_connection, mut tunnel)) =
            crate::desktop::connect::open_record(&record, &keys).await
        {
            let mut opening = vec![crate::control::fleet::removal::PURPOSE_SSH];
            opening.extend_from_slice(&crate::control::fleet::removal::prove(
                b.fleet.key(),
                &tunnel.hash,
            ));
            let _ = tunnel.io.write_all(&opening).await;
            proved = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert!(proved, "never let through");
    until(10, || a.fleet.inner().proven.contains_key(&b.fleet.me())).await;
    assert!(a.state.auth().devices().get(&legacy).is_some());
    // Removed from the fleet: the pairing whose key it proved goes too.
    a.ask(Ask::Remove {
        member: b.me(),
        also_members: Vec::new(),
        also_hosts: Vec::new(),
        reason: crate::control::fleet::Reason::Compromised,
    });
    until(10, || a.state.auth().devices().get(&legacy).is_none()).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_removed_member_takes_pairings_with_every_key_it_proved() {
    use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
    let (http, _, _) = relay().await;
    let (a, b) = (Node::new(http, 1).await, Node::new(http, 2).await);
    assert_eq!(a.ask(Ask::Found), Answer::Done);
    join(&a, &b);
    // A desktop paired here with a tunnel key b proved before it rotated.
    let old = [0x5a; 32];
    let point = SigningKey::from_slice(&[9; 32])
        .unwrap()
        .verifying_key()
        .to_sec1_point(false);
    let legacy = crate::control::DeviceId("legacy".into());
    a.state
        .auth()
        .devices_mut()
        .add(
            crate::auth::NewDevice {
                app: None,
                fleet: None,
                id: legacy.clone(),
                name: "b before it rotated".into(),
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
                    tunnel: URL_SAFE_NO_PAD.encode(old),
                }),
                ssh: Some(
                    crate::ssh_access::SshKey::parse(
                        &crate::desktop::keys::ssh_public_key(b.dir.path()).unwrap(),
                    )
                    .unwrap(),
                ),
            },
            now_ms(),
        )
        .unwrap();
    a.fleet.proved(b.fleet.me(), old).unwrap();
    a.fleet.proved(b.fleet.me(), [0x6b; 32]).unwrap();
    a.ask(Ask::Remove {
        member: b.me(),
        also_members: Vec::new(),
        also_hosts: Vec::new(),
        reason: crate::control::fleet::Reason::Lost,
    });
    grants::apply(&a.state, &a.fleet).unwrap();
    assert!(a.state.auth().devices().get(&legacy).is_none());
    // Forgotten on disk too.
    assert!(
        Fleet::load(a.dir.path(), now_ms())
            .unwrap()
            .inner()
            .proven
            .is_empty()
    );
}
