//! Fleet members on a real relay on loopback: an allowed member reaches
//! SSH and a denied one does not; a removal another member passes in the
//! tunnel suspends its target at once, and the next access file confirms
//! it or, when the log voided it, lifts it.

use super::*;
use crate::{
    access::{Access, Id, Member},
    authorized_keys,
    fleet::Held,
    removal::{self, Message, PROOF_LEN, PURPOSE_REMOVAL, PURPOSE_SSH, tests::remove_op},
};

pub(super) const FLEET: [u8; 32] = [0xf1; 32];

/// The fleet key of the member whose device is `desktop`.
fn fleet_key(desktop: &Desktop) -> SigningKey {
    let mut secret = desktop.tunnel.identity().x25519();
    secret[0] &= 0x7f;
    SigningKey::from_slice(&secret).unwrap()
}

/// `desktop` as fleet member `seed`.
pub(super) fn member(desktop: &Desktop, seed: u8, allow: bool, rev: u64) -> (Id, Member) {
    let member = Member {
        allow,
        dial: client::public(&desktop.dial),
        fleet_key: client::public(&fleet_key(desktop)),
        name: format!("computer{seed}"),
        rev,
        ssh: Some(authorized_keys::tests::key(seed)),
        tunnel: desktop.tunnel.identity().x25519(),
        keys: 0,
        tiebreak: Id::default(),
        former: Vec::new(),
    };
    (Id([seed; 32]), member)
}

/// Applies an access file to `dir`, as `fleet apply` does, and has the
/// running agent read it at once rather than at its next poll.
pub(super) fn apply(
    running: &Running,
    dir: &Path,
    members: Vec<(Id, Member)>,
    removed: &[u8],
    settled: &[[u8; 32]],
) -> Held {
    let access = Access {
        fleet: Id(FLEET),
        members: members.into_iter().collect(),
        removed: removed.iter().map(|seed| Id([*seed; 32])).collect(),
        paused: false,
        settled: settled.iter().copied().map(Id).collect(),
    };
    let held = Held::apply(dir, Some(&dir.join("authorized_keys")), access).unwrap();
    running.agent.reload().unwrap();
    held
}

/// The members with a line in `dir`'s `authorized_keys`, by short id.
pub(super) fn lines(dir: &Path) -> Vec<String> {
    std::fs::read_to_string(dir.join("authorized_keys"))
        .unwrap_or_default()
        .lines()
        .filter_map(authorized_keys::fleet_tag)
        .map(|tag| tag.split(':').nth(1).unwrap().to_owned())
        .collect()
}

/// Dials as `desktop` and says what for.
pub(super) async fn open(running: &Running, desktop: &Desktop, purpose: u8) -> Session {
    open_as(running, desktop, purpose, &fleet_key(desktop)).await
}

/// Dials as `desktop`, says what for, and proves it with fleet key `key`.
async fn open_as(running: &Running, desktop: &Desktop, purpose: u8, key: &SigningKey) -> Session {
    let mut session = dial(running, desktop).await.unwrap();
    let mut opening = vec![purpose];
    opening.extend_from_slice(&removal::prove(key, &session.hash));
    session.io.write_all(&opening).await.unwrap();
    session
}

/// Passes `ops` as `desktop` and reads the answer.
async fn pass(running: &Running, desktop: &Desktop, ops: Vec<Vec<u8>>) -> Message<[u8; 32]> {
    let mut session = open(running, desktop, PURPOSE_REMOVAL).await;
    let frame = Message::<[u8; 32]>::Removal(ops).frame().unwrap();
    session.io.write_all(&frame).await.unwrap();
    let mut length = [0; 4];
    tokio::time::timeout(Duration::from_secs(5), session.io.read_exact(&mut length))
        .await
        .expect("no answer")
        .unwrap();
    let mut bytes = vec![0; removal::frame_length(length).unwrap()];
    session.io.read_exact(&mut bytes).await.unwrap();
    Message::decode(&bytes).unwrap()
}

/// Whether the session ends without a word, within five seconds.
pub(super) async fn closes(io: &mut tokio::io::DuplexStream) -> bool {
    let mut rest = Vec::new();
    tokio::time::timeout(Duration::from_secs(5), io.read_to_end(&mut rest))
        .await
        .is_ok()
}

/// Waits, up to five seconds, until the relay turns `desktop` away.
async fn until_offline(running: &Running, desktop: &Desktop) {
    let connection = client::connect(running.quic, running.pin).await.unwrap();
    for _ in 0..100 {
        let dialed = client::dial(&connection, running.agent.route(), &desktop.dial).await;
        if matches!(dialed, Err(client::Error::Refused(Status::Offline))) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("the relay still lets it dial");
}

fn removal_id(op: &[u8]) -> [u8; 32] {
    removal::peek(op).unwrap().unwrap().id
}

#[tokio::test(flavor = "multi_thread")]
async fn an_allowed_member_reaches_ssh_and_a_denied_one_does_not() {
    let dir = tempfile::tempdir().unwrap();
    let running = start(dir.path()).await;
    let (a, b) = (Desktop::new(0x51), Desktop::new(0x52));
    apply(
        &running,
        dir.path(),
        vec![member(&a, 1, true, 0), member(&b, 2, false, 1)],
        &[],
        &[],
    );
    assert_eq!(lines(dir.path()), [Id([1; 32]).short()]);

    let mut allowed = open(&running, &a, PURPOSE_SSH).await;
    assert_eq!(greeting(&mut allowed.io).await, b"SSH-2.0-test\r\n");
    let mut denied = open(&running, &b, PURPOSE_SSH).await;
    assert!(closes(&mut denied.io).await, "a denied member reached SSH");
    until_offline(&running, &Desktop::new(0x53)).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_suspended_members_keys_never_reach_ssh_as_an_enrolled_desktop() {
    let dir = tempfile::tempdir().unwrap();
    let (a, laptop) = (Desktop::new(0x51), Desktop::new(0x52));
    authorize(dir.path(), &[&laptop]);
    let running = start(dir.path()).await;
    apply(
        &running,
        dir.path(),
        vec![member(&a, 1, true, 0), member(&laptop, 2, true, 0)],
        &[],
        &[],
    );
    let op = remove_op(FLEET, [1; 32], &[[2; 32]]);
    pass(&running, &a, vec![op]).await;
    // Neither as the member nor as the desktop enrolled with its keys.
    until_offline(&running, &laptop).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_desktop_enrolled_one_by_one_keeps_its_session_beside_the_fleet() {
    let dir = tempfile::tempdir().unwrap();
    let desktop = Desktop::new(0x31);
    authorize(dir.path(), &[&desktop]);
    let running = start(dir.path()).await;
    apply(
        &running,
        dir.path(),
        vec![member(&Desktop::new(0x51), 1, true, 0)],
        &[],
        &[],
    );
    // No purpose byte: the session is SSH from the first byte.
    let mut session = dial(&running, &desktop).await.unwrap();
    assert_eq!(greeting(&mut session.io).await, b"SSH-2.0-test\r\n");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_removal_suspends_at_once_and_the_next_access_file_confirms_it() {
    let dir = tempfile::tempdir().unwrap();
    let running = start(dir.path()).await;
    let (a, b) = (Desktop::new(0x51), Desktop::new(0x52));
    let members = || vec![member(&a, 1, true, 0), member(&b, 2, true, 0)];
    apply(&running, dir.path(), members(), &[], &[]);
    let mut live = open(&running, &b, PURPOSE_SSH).await;
    greeting(&mut live.io).await;

    let op = remove_op(FLEET, [1; 32], &[[2; 32]]);
    assert_eq!(
        pass(&running, &a, vec![op.clone()]).await,
        Message::Held(vec![removal_id(&op)])
    );
    assert!(
        closes(&mut live.io).await,
        "the suspended member's session lived on"
    );
    assert_eq!(lines(dir.path()), [Id([1; 32]).short()]);
    until_offline(&running, &b).await;

    // The log confirms it: 2 is removed for good.
    let held = apply(
        &running,
        dir.path(),
        vec![member(&a, 1, true, 0)],
        &[2],
        &[],
    );
    assert!(held.suspended.is_empty());
    // A member that never heard of the removal cannot undo it.
    apply(
        &running,
        dir.path(),
        vec![member(&a, 1, true, 0), member(&b, 2, true, 9)],
        &[],
        &[],
    );
    until_offline(&running, &b).await;
    assert_eq!(lines(dir.path()), [Id([1; 32]).short()]);
}

#[tokio::test(flavor = "multi_thread")]
async fn the_next_access_file_lifts_a_suspension_the_log_voided() {
    let dir = tempfile::tempdir().unwrap();
    let running = start(dir.path()).await;
    let (a, b) = (Desktop::new(0x51), Desktop::new(0x52));
    let members = || vec![member(&a, 1, true, 0), member(&b, 2, true, 0)];
    apply(&running, dir.path(), members(), &[], &[]);
    let op = remove_op(FLEET, [1; 32], &[[2; 32]]);
    pass(&running, &a, vec![op.clone()]).await;
    until_offline(&running, &b).await;

    let held = apply(&running, dir.path(), members(), &[], &[removal_id(&op)]);
    assert!(held.suspended.is_empty());
    assert_eq!(
        lines(dir.path()),
        [Id([1; 32]).short(), Id([2; 32]).short()]
    );
    let mut session = open(&running, &b, PURPOSE_SSH).await;
    assert_eq!(greeting(&mut session.io).await, b"SSH-2.0-test\r\n");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_member_cannot_suspend_itself_and_a_stranger_passes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let running = start(dir.path()).await;
    let a = Desktop::new(0x51);
    apply(&running, dir.path(), vec![member(&a, 1, true, 0)], &[], &[]);
    let op = remove_op(FLEET, [1; 32], &[[1; 32]]);
    // Taken, so the sender owes it no more, but acted on by someone else.
    assert_eq!(
        pass(&running, &a, vec![op.clone()]).await,
        Message::Held(vec![removal_id(&op)])
    );
    let mut session = open(&running, &a, PURPOSE_SSH).await;
    assert_eq!(greeting(&mut session.io).await, b"SSH-2.0-test\r\n");
    until_offline(&running, &Desktop::new(0x53)).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_session_with_another_keys_proof_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let running = start(dir.path()).await;
    let (a, b) = (Desktop::new(0x51), Desktop::new(0x52));
    apply(
        &running,
        dir.path(),
        vec![member(&a, 1, true, 0), member(&b, 2, true, 0)],
        &[],
        &[],
    );
    // a's device, proving with b's fleet key: no SSH.
    let mut wrong = open_as(&running, &a, PURPOSE_SSH, &fleet_key(&b)).await;
    assert!(closes(&mut wrong.io).await, "a wrong proof reached SSH");
    // Nor a removal: nothing is answered and no one is suspended.
    let mut wrong = open_as(&running, &a, PURPOSE_REMOVAL, &fleet_key(&b)).await;
    let op = remove_op(FLEET, [1; 32], &[[2; 32]]);
    let frame = Message::<[u8; 32]>::Removal(vec![op]).frame().unwrap();
    let _ = wrong.io.write_all(&frame).await;
    assert!(closes(&mut wrong.io).await, "a wrong proof was answered");
    assert!(Held::load(dir.path()).unwrap().suspended.is_empty());
    // With its own key, the same member reaches SSH.
    let mut right = open(&running, &a, PURPOSE_SSH).await;
    assert_eq!(greeting(&mut right.io).await, b"SSH-2.0-test\r\n");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_session_that_sends_no_proof_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let running = start(dir.path()).await;
    let a = Desktop::new(0x51);
    apply(&running, dir.path(), vec![member(&a, 1, true, 0)], &[], &[]);
    let mut session = dial(&running, &a).await.unwrap();
    // The purpose byte, then an SSH greeting where the proof belongs.
    session.io.write_all(&[PURPOSE_SSH]).await.unwrap();
    session.io.write_all(&[b'S'; PROOF_LEN]).await.unwrap();
    assert!(
        closes(&mut session.io).await,
        "an unproven session reached SSH"
    );
}
