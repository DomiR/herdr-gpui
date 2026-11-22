//! A removal this computer could not save is not acknowledged, takes
//! effect at once, and is saved by the next exchange that carries it.

use super::*;
use crate::control::fleet::{Reason, Remove, removal::Message};

#[tokio::test(flavor = "multi_thread")]
async fn an_unsaved_removal_is_not_held_until_it_is_saved() {
    let (http, _, _) = relay().await;
    let (a, mut b, c) = (
        Node::new(http, 1).await,
        Node::new(http, 2).await,
        Node::new(http, 3).await,
    );
    a.ask(Ask::Found);
    join(&a, &b);
    join(&a, &c);
    b.fleet.receive(a.ops(), now_ms()).unwrap();
    b.start().await;
    until(10, || b.lines().len() == 2).await;
    let removal = a
        .fleet
        .sign(
            Body::Remove(Remove {
                member: c.fleet.me(),
                also_members: Vec::new(),
                also_hosts: Vec::new(),
                reason: Reason::Compromised,
            }),
            now_ms(),
        )
        .unwrap();
    // b's log cannot be written: a directory stands where it goes.
    let log = b.dir.path().join("fleet/log.bin");
    std::fs::remove_file(&log).unwrap();
    std::fs::create_dir(&log).unwrap();
    std::fs::write(log.join("blocker"), b"").unwrap();
    let changed = b.fleet.watch();
    let mut tunnel = flood::exchange(&a, &b).await;
    flood::send(&mut tunnel, &Message::Removal(vec![removal.encode()])).await;
    let mut byte = [0];
    let ended = tokio::io::AsyncReadExt::read(&mut tunnel.io, &mut byte).await;
    assert!(matches!(ended, Ok(0) | Err(_)), "acknowledged unsaved");
    // Cut off at once all the same.
    assert!(changed.has_changed().unwrap());
    until(10, || b.lines().len() == 1).await;
    assert!(b.fleet.is_unsaved());
    // Freed, the same removal again: saved, then held.
    std::fs::remove_dir_all(&log).unwrap();
    let mut tunnel = flood::exchange(&a, &b).await;
    flood::send(&mut tunnel, &Message::Removal(vec![removal.encode()])).await;
    assert_eq!(
        flood::receive(&mut tunnel).await,
        Message::Held(vec![removal.id()])
    );
    assert!(!b.fleet.is_unsaved());
    let again = Fleet::load(b.dir.path(), now_ms()).unwrap();
    assert!(!again.inner().log.as_ref().unwrap().is_member(&c.fleet.me()));
}
