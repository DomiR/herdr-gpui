//! `fleet apply` with no daemon running takes the fleet folder's lock, so
//! it never writes the log under a daemon starting or another update.

use super::*;
use crate::control::fleet::sync::Apply;

/// b, admitted by a, and the operations that admit it.
async fn admitted() -> (Node, Node, Apply) {
    let (http, _, _) = relay().await;
    let (a, b) = (Node::new(http, 1).await, Node::new(http, 2).await);
    assert_eq!(a.ask(Ask::Found), Answer::Done);
    let fleet = a.view().fleet;
    let Answer::Bundle { bundle, signature } = b.ask(Ask::Bundle {
        fleet: fleet.clone(),
    }) else {
        panic!("no bundle");
    };
    a.ask(Ask::Admit {
        bundle,
        signature,
        method: crate::control::fleet::Method::Ssh,
    });
    let apply = Apply {
        fleet: Some(fleet),
        heads: Vec::new(),
        ops: sync::encode_ops(&a.ops()),
        nonce: None,
        peer: None,
    };
    (a, b, apply)
}

#[tokio::test(flavor = "multi_thread")]
async fn an_offline_update_waits_for_the_lock() {
    let (_a, b, apply) = admitted().await;
    let dir = b.dir.path().to_owned();
    let held = lock::Lock::try_acquire(&dir).unwrap().unwrap();
    let waiting = {
        let dir = dir.clone();
        std::thread::spawn(move || run_apply(&dir, apply))
    };
    std::thread::sleep(Duration::from_millis(300));
    assert!(!waiting.is_finished());
    // Nothing written while another holds the lock.
    assert!(!dir.join("fleet/log.bin").exists());
    drop(held);
    let applied = waiting.join().unwrap().unwrap();
    assert_eq!(applied.refused, 0);
    assert!(Fleet::load(&dir, now_ms()).unwrap().inner().log.is_some());
}

#[tokio::test(flavor = "multi_thread")]
async fn the_daemon_holds_the_lock_while_it_lives() {
    let dir = tempfile::tempdir().unwrap();
    let dir = dir.path();
    let daemon = Fleet::load_exclusive(dir, now_ms()).unwrap();
    assert!(lock::Lock::try_acquire(dir).unwrap().is_none());
    drop(daemon);
    // A process another test forks in the meantime shares the lock's open
    // file until it execs, so the release can lag for a moment.
    until(5, || lock::Lock::try_acquire(dir).unwrap().is_some()).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn concurrent_offline_updates_keep_each_others_operations() {
    let (a, b, first) = admitted().await;
    // a moves on: the second update carries one more operation.
    let host = a
        .view()
        .hosts
        .iter()
        .find(|host| host.added_by == a.me())
        .unwrap()
        .id
        .clone();
    a.ask(Ask::Access {
        host,
        member: b.me(),
        allow: false,
    });
    let second = Apply {
        fleet: first.fleet.clone(),
        heads: Vec::new(),
        ops: sync::encode_ops(&a.ops()),
        nonce: None,
        peer: None,
    };
    let dir = b.dir.path().to_owned();
    let updates: Vec<_> = [first, second]
        .into_iter()
        .map(|apply| {
            let dir = dir.clone();
            std::thread::spawn(move || run_apply(&dir, apply))
        })
        .collect();
    for update in updates {
        update.join().unwrap().unwrap();
    }
    let reread = Fleet::load(&dir, now_ms()).unwrap();
    assert_eq!(
        reread.inner().log.as_ref().unwrap().heads(),
        a.fleet.inner().log.as_ref().unwrap().heads()
    );
}
