//! `fleet scrub` against what runs beside it: a starting daemon or an
//! offline update holding the fleet lock, a grants write already under
//! way, and the SSH session that asked for the scrub.

use super::*;
use std::sync::atomic::{AtomicBool, Ordering};

#[test]
fn an_offline_scrub_waits_for_the_fleet_lock() {
    let dir = tempfile::tempdir().unwrap();
    // A daemon starting, or an update writing, holds the lock.
    let held = lock::Lock::try_acquire(dir.path()).unwrap().unwrap();
    let ran = AtomicBool::new(false);
    std::thread::scope(|scope| {
        let scrub = scope.spawn(|| {
            scrub_with(dir.path(), None, |state_dir| {
                // Run only while holding the lock itself.
                assert!(lock::Lock::try_acquire(state_dir).unwrap().is_none());
                ran.store(true, Ordering::SeqCst);
                Ok(())
            })
        });
        std::thread::sleep(Duration::from_millis(300));
        assert!(
            !ran.load(Ordering::SeqCst),
            "nothing is forgotten under the lock"
        );
        drop(held);
        scrub.join().unwrap().unwrap();
    });
    assert!(ran.load(Ordering::SeqCst));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_scrub_waits_for_a_grants_write_under_way() {
    let (http, _, _) = relay().await;
    let (a, b) = (Node::new(http, 1).await, Node::new(http, 2).await);
    assert_eq!(a.ask(Ask::Found), Answer::Done);
    join(&a, &b);
    let log = b.dir.path().join("fleet/log.bin");
    // A grants write has read the fleet and not yet written it.
    let writing = b.fleet.writing();
    let (state, fleet) = (b.state.clone(), b.fleet.clone());
    let scrub = tokio::task::spawn_blocking(move || {
        ask::answer(&state, &fleet, Ask::Scrub { session: None }, now_ms())
    });
    // Blocking on purpose: the guard stands for a write on a blocking
    // thread, never held across an await.
    std::thread::sleep(Duration::from_millis(300));
    assert!(log.exists(), "the scrub waits for the write to finish");
    drop(writing);
    assert_eq!(scrub.await.unwrap().unwrap(), Answer::Done);
    assert!(!log.exists());
    // A write after it finds no fleet, and grants nothing.
    grants::apply(&b.state, &b.fleet).unwrap();
    assert!(b.lines().is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn only_the_session_that_asked_for_a_scrub_may_read_its_answer() {
    let (http, _, _) = relay().await;
    let (a, b) = (Node::new(http, 1).await, Node::new(http, 2).await);
    assert_eq!(a.ask(Ask::Found), Answer::Done);
    join(&a, &b);
    assert_eq!(b.fleet.scrub_grace(4022), None);
    assert_eq!(
        b.ask(Ask::Scrub {
            session: Some(4022)
        }),
        Answer::Done
    );
    let left = b.fleet.scrub_grace(4022).unwrap();
    assert!(left > Duration::ZERO && left <= SCRUB_GRACE);
    // Any other session ends at once.
    assert_eq!(b.fleet.scrub_grace(4023), None);
    // In a fleet again, no session keeps anything.
    assert_eq!(b.ask(Ask::Found), Answer::Done);
    assert_eq!(b.fleet.scrub_grace(4022), None);
}

#[test]
fn a_fleet_session_is_named_by_its_loopback_client_port() {
    assert_eq!(
        loopback_client_port("127.0.0.1 51234 127.0.0.1 22"),
        Some(51234)
    );
    assert_eq!(loopback_client_port("::1 51234 ::1 22"), Some(51234));
    // A session from elsewhere is no fleet session.
    assert_eq!(loopback_client_port("192.0.2.7 51234 192.0.2.1 22"), None);
    assert_eq!(loopback_client_port("garbage"), None);
}

#[test]
fn a_scrub_with_no_session_keeps_its_wire_shape() {
    // An older Sheprd asks without a session, and gets the same answer.
    let older: Ask = serde_json::from_str(r#"{"ask":"scrub"}"#).unwrap();
    assert_eq!(older, Ask::Scrub { session: None });
    assert_eq!(
        serde_json::to_string(&Ask::Scrub { session: None }).unwrap(),
        r#"{"ask":"scrub"}"#
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_proof_after_a_scrub_is_not_kept() {
    let (http, _, _) = relay().await;
    let (a, b) = (Node::new(http, 1).await, Node::new(http, 2).await);
    assert_eq!(a.ask(Ask::Found), Answer::Done);
    join(&a, &b);
    assert!(b.fleet.proved(a.fleet.me(), [0x31; 32]).unwrap());
    assert_eq!(b.ask(Ask::Scrub { session: None }), Answer::Done);
    // A removal exchange that proved a key while the scrub ran keeps
    // nothing: its caller revokes the pairing itself.
    assert!(!b.fleet.proved(a.fleet.me(), [0x32; 32]).unwrap());
    assert!(b.fleet.proven_keys().is_empty());
}

#[test]
fn keys_an_earlier_build_recorded_are_not_revoked_by_a_scrub() {
    let dir = tempfile::tempdir().unwrap();
    let folder = dir.path().join("fleet");
    std::fs::create_dir_all(&folder).unwrap();
    let member = "ab".repeat(32);
    let key = "cd".repeat(32);
    // An earlier build recorded keys on a handshake alone: any member's
    // bundle could have named them.
    std::fs::write(
        folder.join("peers.json"),
        format!(r#"{{"proven":{{"{member}":"{key}"}}}}"#),
    )
    .unwrap();
    assert!(
        crate::fleet::store::proven_keys(dir.path())
            .unwrap()
            .is_empty()
    );
    // Recorded by a proven session, they count.
    std::fs::write(
        folder.join("peers.json"),
        format!(r#"{{"proven":{{"{member}":["{key}"]}},"proven_version":2}}"#),
    )
    .unwrap();
    assert_eq!(
        crate::fleet::store::proven_keys(dir.path()).unwrap(),
        vec![[0xcd; 32]]
    );
}

#[test]
fn a_scrub_that_cannot_find_the_ssh_keys_fails() {
    use crate::ssh_access::{AuthorizedKeys, Error as SshError};
    // The daemon started with no usable HOME: lines an earlier run wrote
    // may remain, so the scrub refuses rather than report access gone.
    assert!(matches!(
        crate::fleet::scrub::located(None, || Err(SshError::NoHome)),
        Err(Error::SshAccess(SshError::NoHome))
    ));
    // Where the fleet never writes lines, there are none to remove.
    assert!(matches!(
        crate::fleet::scrub::located(None, || Err(SshError::Unsupported)),
        Ok(None)
    ));
    // The daemon's own file wins; the user's is the fallback.
    let file = PathBuf::from("/nowhere/authorized_keys");
    assert!(
        crate::fleet::scrub::located(None, || Ok(AuthorizedKeys::at(file.clone())))
            .unwrap()
            .is_some()
    );
    assert!(
        crate::fleet::scrub::located(Some(AuthorizedKeys::at(file)), || Err(SshError::NoHome))
            .unwrap()
            .is_some()
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_removal_exchange_open_across_a_scrub_cannot_restore_the_fleet() {
    let (http, _, _) = relay().await;
    let (a, b) = (Node::new(http, 1).await, Node::new(http, 2).await);
    assert_eq!(a.ask(Ask::Found), Answer::Done);
    join(&a, &b);
    // The exchange began while b was in a's fleet.
    let serving = b.fleet.inner().log.as_ref().unwrap().fleet();
    b.fleet.receive_in(serving, a.ops(), now_ms()).unwrap();
    assert_eq!(b.ask(Ask::Scrub { session: None }), Answer::Done);
    // Its history, which admits b's key, joins nothing now.
    assert!(matches!(
        b.fleet.receive_in(serving, a.ops(), now_ms()),
        Err(Error::NoFleet)
    ));
    assert!(b.fleet.inner().log.is_none());
    assert!(ask::view(&b.fleet).is_none());
    assert!(b.lines().is_empty());
}

/// `a` founds a fleet that shares `host`, which takes the log as a host.
async fn shared_host() -> (Node, Node) {
    use crate::control::fleet::sync::Apply;
    let (http, _, _) = relay().await;
    let (a, host) = (Node::new(http, 1).await, Node::new(http, 5).await);
    assert_eq!(a.ask(Ask::Found), Answer::Done);
    let Answer::Intro { intro } = host.ask(Ask::Intro) else {
        panic!("no intro");
    };
    assert_eq!(
        a.ask(Ask::AddHost {
            intro,
            deny: Vec::new(),
        }),
        Answer::Done
    );
    let Answer::Applied(_) = host.ask(Ask::Apply(Apply {
        heads: a.view().heads,
        ops: sync::encode_ops(&a.ops()),
        ..Apply::default()
    })) else {
        panic!("not applied");
    };
    assert!(host.fleet.host().is_some());
    (a, host)
}

#[tokio::test(flavor = "multi_thread")]
async fn a_scrub_forgets_that_this_computer_was_a_shared_host() {
    use crate::control::fleet::Reason;
    let (a, host) = shared_host().await;
    assert_eq!(host.ask(Ask::Scrub { session: None }), Answer::Done);
    // The role went with the fleet, here and on disk.
    assert_eq!(host.fleet.host(), None);
    assert!(!host.dir.path().join("fleet/hosted").exists());
    assert_eq!(Fleet::load(host.dir.path(), now_ms()).unwrap().host(), None);
    // Joining as a member later, and leaving, takes a member's path: the
    // log stays until the removal is passed on.
    join(&a, &host);
    assert_eq!(
        host.ask(Ask::Remove {
            member: host.me(),
            also_members: Vec::new(),
            also_hosts: Vec::new(),
            reason: Reason::Left,
        }),
        Answer::Done
    );
    assert!(host.fleet.in_fleet().unwrap());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_shared_host_role_that_cannot_go_fails_the_scrub_and_stays() {
    let (_a, host) = shared_host().await;
    // The role's file cannot be removed.
    let hosted = host.dir.path().join("fleet/hosted");
    std::fs::remove_file(&hosted).unwrap();
    std::fs::create_dir_all(hosted.join("stuck")).unwrap();
    assert!(
        ask::answer(
            &host.state,
            &host.fleet,
            Ask::Scrub { session: None },
            now_ms()
        )
        .is_err()
    );
    // Nothing went: the role and the fleet stay, and asking again after
    // the file is fixed finishes the job.
    assert!(host.fleet.host().is_some());
    assert!(host.fleet.inner().log.is_some());
}
