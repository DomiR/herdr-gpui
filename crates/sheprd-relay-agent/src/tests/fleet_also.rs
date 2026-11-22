//! A removal passed with the members the sender's log removes beside
//! those it names: the agent suspends them too, in the sender's name
//! alone, until the next access file confirms or settles the removal.

use super::fleet::{FLEET, apply, member, open};
use super::*;
use crate::{
    access::Id,
    fleet::Held,
    removal::{self, Message, PURPOSE_REMOVAL, peek, tests::remove_op},
};

/// Sends `also` then `ops` as `desktop` and reads the answer.
async fn pass_with(
    running: &Running,
    desktop: &Desktop,
    also: Vec<([u8; 32], [u8; 32])>,
    ops: Vec<Vec<u8>>,
) -> Message<[u8; 32]> {
    let mut session = open(running, desktop, PURPOSE_REMOVAL).await;
    let also = Message::<[u8; 32]>::Also(also).frame().unwrap();
    session.io.write_all(&also).await.unwrap();
    let ops = Message::<[u8; 32]>::Removal(ops).frame().unwrap();
    session.io.write_all(&ops).await.unwrap();
    let mut length = [0; 4];
    tokio::time::timeout(Duration::from_secs(5), session.io.read_exact(&mut length))
        .await
        .expect("no answer")
        .unwrap();
    let mut bytes = vec![0; removal::frame_length(length).unwrap()];
    session.io.read_exact(&mut bytes).await.unwrap();
    Message::decode(&bytes).unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn members_a_removal_takes_beyond_those_it_names_are_suspended_too() {
    let dir = tempfile::tempdir().unwrap();
    let running = start(dir.path()).await;
    // a removes c; c had admitted x at the same time, so the log removes
    // x with it, though the removal names only c.
    let (a, c, x) = (Desktop::new(0x51), Desktop::new(0x52), Desktop::new(0x53));
    let members = vec![
        member(&a, 1, true, 0),
        member(&c, 2, true, 0),
        member(&x, 3, true, 0),
    ];
    apply(&running, dir.path(), members, &[], &[]);
    let op = remove_op(FLEET, [1; 32], &[[2; 32]]);
    let id = peek(&op).unwrap().unwrap().id;
    let answer = pass_with(&running, &a, vec![(id, [3; 32])], vec![op]).await;
    assert_eq!(answer, Message::Held(vec![id]));
    let held = Held::load(dir.path()).unwrap();
    assert!(held.suspended.contains_key(&Id([2; 32])));
    assert!(held.suspended.contains_key(&Id([3; 32])), "x keeps access");
    assert!(held.suspended[&Id([3; 32])].removals.contains_key(&Id(id)));
    // The next access file confirms it: x is removed for good.
    let members = vec![member(&a, 1, true, 0)];
    let held = apply(&running, dir.path(), members, &[2, 3], &[id]);
    assert!(held.suspended.is_empty());
    assert!(held.access.unwrap().removed.contains(&Id([3; 32])));
}

#[tokio::test(flavor = "multi_thread")]
async fn extra_members_for_a_removal_the_sender_did_not_sign_are_not_suspended() {
    let dir = tempfile::tempdir().unwrap();
    let running = start(dir.path()).await;
    let (a, b, x) = (Desktop::new(0x51), Desktop::new(0x52), Desktop::new(0x53));
    let members = vec![
        member(&a, 1, true, 0),
        member(&b, 2, true, 0),
        member(&x, 3, true, 0),
    ];
    apply(&running, dir.path(), members, &[], &[]);
    // A removal b signed, passed by a with x said to go with it.
    let op = remove_op(FLEET, [2; 32], &[[1; 32]]);
    let id = peek(&op).unwrap().unwrap().id;
    pass_with(&running, &a, vec![(id, [3; 32])], vec![op]).await;
    let held = Held::load(dir.path()).unwrap();
    assert!(held.suspended.is_empty(), "{:?}", held.suspended);
}

#[test]
fn an_also_message_reads_back_and_is_bounded() {
    let pairs: Vec<([u8; 32], [u8; 32])> = vec![([1; 32], [2; 32]), ([1; 32], [3; 32])];
    let message = Message::<[u8; 32]>::Also(pairs);
    let bytes = message.encode().unwrap();
    assert_eq!(Message::decode(&bytes).unwrap(), message);
    let too_many = Message::<[u8; 32]>::Also(vec![([0; 32], [0; 32]); removal::MAX_ALSO + 1]);
    assert!(too_many.encode().is_err());
    let mut cut = bytes.clone();
    cut.pop();
    assert!(Message::<[u8; 32]>::decode(&cut).is_err());
}
