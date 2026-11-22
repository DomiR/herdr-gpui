//! However many removals a member sends whose history never comes, the
//! suspensions kept stay bounded and `peers.json` readable. Folded ones
//! still settle by their own history, end on their own time, and stand
//! by their own removal's signer rule.

use super::*;
use crate::control::fleet::{
    Access, MAX_COMPUTERS, MAX_OP, MAX_PHONES, MAX_TOMBSTONES, Reason, Remove,
};
use suspensions::{
    FoldedRemoval, FoldedTarget, MAX_FOLDED_IDS, MAX_UNCONFIRMED_PER_SIGNER, Overflow,
};

const DAY: u64 = 24 * 60 * 60 * 1000;

/// a, b, c, and d in one fleet, none started.
async fn four() -> (Node, Node, Node, Node) {
    let (a, b, c) = idle().await;
    let d = Node::new("127.0.0.1:9".parse().unwrap(), 4).await;
    join(&a, &d);
    for node in [&b, &c] {
        node.fleet.receive(a.ops(), now_ms()).unwrap();
    }
    (a, b, c, d)
}

/// `by`'s removal of `member` on everything `by` holds, kept out of its
/// own log, with `seq` when given.
fn removal(by: &Node, member: &Node, seq: Option<u64>) -> Op {
    let inner = by.fleet.inner();
    let mut unsigned = inner.log.as_ref().unwrap().draft(
        by.fleet.me(),
        now_ms(),
        Body::Remove(Remove {
            member: member.fleet.me(),
            also_members: Vec::new(),
            also_hosts: Vec::new(),
            reason: Reason::Compromised,
        }),
    );
    if let Some(seq) = seq {
        unsigned.seq = seq;
    }
    unsigned.sign(by.fleet.key()).unwrap()
}

/// An operation `by` signs and keeps, which no one else has seen.
fn unseen(by: &Node, turn: usize) -> Op {
    let host = grants::own_host(
        by.fleet.inner().log.as_ref().unwrap().state(),
        &by.fleet.me(),
        None,
    )
    .unwrap();
    by.fleet
        .sign(
            Body::Access(Access {
                host,
                member: by.fleet.me(),
                allow: turn % 2 == 1,
                rev: u64::try_from(turn).unwrap() + 100,
            }),
            now_ms(),
        )
        .unwrap()
}

/// `count` of `signer`'s removals of `targets`, kept apart, arrived at
/// `since_ms`, with ids from `first`.
fn kept(
    inner: &mut Inner,
    signer: MemberId,
    targets: &[MemberId],
    since_ms: u64,
    count: usize,
    first: u8,
) {
    for turn in 0..count {
        inner.unconfirmed.insert(
            OpId([first + u8::try_from(turn).unwrap(); 32]),
            Unconfirmed {
                signer,
                targets: targets.to_vec(),
                since_ms,
                op: None,
            },
        );
    }
}

/// b removes `member`, and `on` hears of it.
fn removed_by_b(b: &Node, member: &Node, on: &Node) {
    let removed = b
        .fleet
        .sign(
            Body::Remove(Remove {
                member: member.fleet.me(),
                also_members: Vec::new(),
                also_hosts: Vec::new(),
                reason: Reason::Compromised,
            }),
            now_ms(),
        )
        .unwrap();
    on.fleet.receive(vec![removed], now_ms()).unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn a_members_many_removals_stay_bounded_and_suspended() {
    let (a, b, c, _) = four().await;
    for turn in 0..2 * MAX_UNCONFIRMED_PER_SIGNER {
        unseen(&a, turn);
        b.fleet
            .receive(vec![removal(&a, &c, None)], now_ms())
            .unwrap();
    }
    {
        let inner = b.fleet.inner();
        assert!(inner.unconfirmed.len() <= MAX_UNCONFIRMED_PER_SIGNER);
        assert!(inner.folded[&a.fleet.me()].contains_key(&c.fleet.me()));
    }
    assert_eq!(b.fleet.suspended(), vec![c.fleet.me()]);
    let again = Fleet::load(b.dir.path(), now_ms()).unwrap();
    assert_eq!(again.suspended(), vec![c.fleet.me()]);
    assert_eq!(again.inner().folded, b.fleet.inner().folded);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_removal_its_history_refuses_lifts_its_target() {
    let (a, b, c, d) = four().await;
    // a's removal of c on an operation b has not seen, reusing that
    // operation's number: its history refuses it.
    let missing = unseen(&a, 0);
    let invalid = removal(&a, &c, Some(missing.unsigned().seq));
    b.fleet.receive(vec![invalid], now_ms()).unwrap();
    for turn in 1..4 {
        unseen(&a, turn);
        b.fleet
            .receive(vec![removal(&a, &d, None)], now_ms() + 1)
            .unwrap();
    }
    let mut cut: Vec<MemberId> = vec![c.fleet.me(), d.fleet.me()];
    cut.sort();
    assert_eq!(b.fleet.suspended(), cut);
    // Its history arrives: refused, and c is let back in; d stays out.
    b.fleet.receive(vec![missing], now_ms() + 2).unwrap();
    assert_eq!(b.fleet.suspended(), vec![d.fleet.me()]);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_newer_fold_never_extends_an_older_suspension() {
    let (a, b, c, d) = four().await;
    let start = now_ms();
    let (signer, older, newer) = (a.fleet.me(), c.fleet.me(), d.fleet.me());
    let mut inner = b.fleet.inner();
    // a's oldest removal cuts c off; the rest, three days on, d.
    kept(&mut inner, signer, &[older], start, 1, 1);
    kept(
        &mut inner,
        signer,
        &[newer],
        start + 3 * DAY,
        MAX_UNCONFIRMED_PER_SIGNER + 1,
        2,
    );
    suspensions::settle(&mut inner, start + 3 * DAY, BTreeSet::new());
    assert_eq!(
        inner.folded[&signer][&older]
            .ids
            .values()
            .map(|removal| removal.since_ms)
            .collect::<Vec<_>>(),
        vec![start]
    );
    // Seven days after the first, c's own time is up, d's is not.
    suspensions::settle(&mut inner, start + 7 * DAY + 1, BTreeSet::new());
    assert!(!inner.folded[&signer].contains_key(&older));
    assert!(inner.folded[&signer].contains_key(&newer));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_folded_target_lasts_as_long_as_its_latest_removal() {
    let (a, b, c, d) = four().await;
    let start = now_ms();
    let (signer, target, other) = (a.fleet.me(), c.fleet.me(), d.fleet.me());
    let mut inner = b.fleet.inner();
    // Two removals of c fold, three days apart; the newest, of d, stay
    // apart.
    kept(&mut inner, signer, &[target], start, 1, 1);
    kept(&mut inner, signer, &[target], start + 3 * DAY, 1, 2);
    kept(
        &mut inner,
        signer,
        &[other],
        start + 4 * DAY,
        MAX_UNCONFIRMED_PER_SIGNER,
        3,
    );
    suspensions::settle(&mut inner, start + 4 * DAY, BTreeSet::new());
    assert_eq!(inner.folded[&signer][&target].ids.len(), 2);
    // A week after the first, the latest still holds it.
    suspensions::settle(&mut inner, start + 7 * DAY + 1, BTreeSet::new());
    assert!(inner.folded[&signer].contains_key(&target));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_folded_target_stands_by_what_its_removals_named() {
    let (a, b, c, d) = four().await;
    removed_by_b(&b, &c, &a);
    let signer = c.fleet.me();
    let now = now_ms();
    let mut inner = a.fleet.inner();
    // Folded from c: d, by a removal that also named b, which removed c,
    // so it stands; a, by one that named a alone, so it does not.
    let folded = |named: &[MemberId]| FoldedTarget {
        ids: BTreeMap::from([(
            OpId([u8::try_from(named.len()).unwrap(); 32]),
            FoldedRemoval {
                since_ms: now,
                named: named.iter().copied().collect(),
                pinned: false,
            },
        )]),
        overflow: None,
    };
    inner.folded.insert(
        signer,
        BTreeMap::from([
            (d.fleet.me(), folded(&[b.fleet.me(), d.fleet.me()])),
            (a.fleet.me(), folded(&[a.fleet.me()])),
        ]),
    );
    suspensions::settle(&mut inner, now, BTreeSet::new());
    assert!(inner.folded[&signer].contains_key(&d.fleet.me()));
    assert!(!inner.folded[&signer].contains_key(&a.fleet.me()));
    // c is no member: the removal that named b stands for good, until
    // its own time ends.
    assert!(
        inner.folded[&signer][&d.fleet.me()]
            .ids
            .values()
            .all(|removal| removal.pinned)
    );
    drop(inner);
    assert_eq!(a.fleet.suspended(), vec![d.fleet.me()]);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_name_ends_with_the_removal_that_gave_it() {
    let (a, b, c, d) = four().await;
    let start = now_ms();
    let signer = c.fleet.me();
    let mut inner = a.fleet.inner();
    // While c is a member, two of its removals fold: one at the start
    // naming b and d, one three days on naming d alone.
    kept(
        &mut inner,
        signer,
        &[b.fleet.me(), d.fleet.me()],
        start,
        1,
        1,
    );
    kept(&mut inner, signer, &[d.fleet.me()], start + 3 * DAY, 1, 2);
    kept(
        &mut inner,
        signer,
        &[b.fleet.me()],
        start + 4 * DAY,
        MAX_UNCONFIRMED_PER_SIGNER,
        3,
    );
    suspensions::settle(&mut inner, start + 4 * DAY, BTreeSet::new());
    drop(inner);
    // b then removes c: d stands only by the first removal, which named b.
    removed_by_b(&b, &c, &a);
    let mut inner = a.fleet.inner();
    suspensions::settle(&mut inner, start + 5 * DAY, BTreeSet::new());
    let held: Vec<u64> = inner.folded[&signer][&d.fleet.me()]
        .ids
        .values()
        .map(|removal| removal.since_ms)
        .collect();
    assert_eq!(held, vec![start]);
    // Once that removal's time is up, the one left never named b: d is
    // let back in, though that one's own time runs on.
    suspensions::settle(&mut inner, start + 7 * DAY + 1, BTreeSet::new());
    assert!(
        inner
            .folded
            .get(&signer)
            .is_none_or(|folded| !folded.contains_key(&d.fleet.me()))
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn acknowledgments_keep_only_removals_sent_and_held() {
    let (a, b, c, d) = four().await;
    let removal = a
        .fleet
        .sign(
            Body::Remove(Remove {
                member: d.fleet.me(),
                also_members: Vec::new(),
                also_hosts: Vec::new(),
                reason: Reason::Compromised,
            }),
            now_ms(),
        )
        .unwrap();
    // A peer saying it holds ids it was never sent: only the one sent
    // counts.
    let made_up: Vec<OpId> = (0..64).map(|seed| OpId([seed; 32])).collect();
    let acked = removals::acknowledged(
        std::slice::from_ref(&removal),
        made_up.iter().copied().chain([removal.id()]).collect(),
    );
    assert_eq!(acked, vec![removal.id()]);
    // What is kept holds only removals the log holds, for members.
    a.fleet
        .held(Holder::Member(b.fleet.me()), made_up, [])
        .unwrap();
    a.fleet
        .held(Holder::Member(c.fleet.me()), acked, [])
        .unwrap();
    a.fleet
        .held(Holder::Member(MemberId([0x77; 32])), [removal.id()], [])
        .unwrap();
    let inner = a.fleet.inner();
    assert!(!inner.held.contains_key(&Holder::Member(b.fleet.me())));
    assert_eq!(
        inner.held[&Holder::Member(c.fleet.me())],
        BTreeSet::from([removal.id()])
    );
    assert!(
        !inner
            .held
            .contains_key(&Holder::Member(MemberId([0x77; 32])))
    );
}

/// The most suspensions held: every live signer's removals kept apart and
/// folded, naming every live member, and every member no longer live
/// with each live member folded and pinned. `variant` sets their ids
/// apart, so two such sets share none.
fn full(variant: u8) -> Suspensions {
    let live = MAX_COMPUTERS + MAX_PHONES;
    let targets: Vec<MemberId> = (0..live)
        .map(|seed| MemberId([u8::try_from(seed).unwrap(); 32]))
        .collect();
    let mut inner = Suspensions::default();
    for seed in 0..live + MAX_TOMBSTONES {
        let mut signer = [0xee; 32];
        signer[..8].copy_from_slice(&u64::try_from(seed).unwrap().to_be_bytes());
        let signer = MemberId(signer);
        let live = seed < live;
        if live {
            for turn in 0..MAX_UNCONFIRMED_PER_SIGNER {
                let mut id = signer.0;
                id[29] = variant;
                id[31] = u8::try_from(turn).unwrap();
                inner.unconfirmed.insert(
                    OpId(id),
                    Unconfirmed {
                        signer,
                        targets: targets.clone(),
                        since_ms: u64::MAX,
                        op: None,
                    },
                );
            }
        }
        let folded = inner.folded.entry(signer).or_default();
        for target in &targets {
            let ids = if live { MAX_FOLDED_IDS } else { 1 };
            folded.insert(
                *target,
                FoldedTarget {
                    ids: (0..ids)
                        .map(|turn| {
                            let mut id = signer.0;
                            id[29] = variant;
                            id[30] = 1;
                            id[31] = u8::try_from(turn).unwrap();
                            let removal = FoldedRemoval {
                                since_ms: u64::MAX,
                                named: if live {
                                    targets.iter().copied().collect()
                                } else {
                                    BTreeSet::new()
                                },
                                pinned: !live,
                            };
                            (OpId(id), removal)
                        })
                        .collect(),
                    overflow: Some(Overflow {
                        since_ms: u64::MAX,
                        named: if live {
                            targets.iter().map(|name| (*name, u64::MAX)).collect()
                        } else {
                            BTreeMap::new()
                        },
                        pinned: (!live).then_some(u64::MAX),
                    }),
                },
            );
        }
    }
    inner
}

#[test]
fn the_most_suspensions_kept_fit_in_the_peers_file() {
    let dir = tempfile::tempdir().unwrap();
    // A save's first write keeps what is held beside what the file held:
    // both full, sharing nothing.
    let merged = crate::fleet::store::merged(&full(1), &full(2));
    for targets in merged.folded.values() {
        assert!(
            targets
                .values()
                .all(|folded| folded.ids.len() <= MAX_FOLDED_IDS)
        );
    }
    let inner = Inner {
        unconfirmed: merged.unconfirmed,
        folded: merged.folded,
        ..Inner::default()
    };
    crate::fleet::store::save(dir.path(), &inner, &mut Default::default()).unwrap();
    let written = std::fs::metadata(dir.path().join("fleet/peers.json"))
        .unwrap()
        .len();
    // The removals kept to check again, at most a full operation each,
    // base64url, with room to spare for their keys and quotes; and what
    // each peer holds, at its bound.
    let removals = u64::try_from(MAX_KEPT_REMOVALS * (MAX_OP * 4 / 3 + 64)).unwrap();
    let held = u64::try_from(MAX_COMPUTERS * (MAX_HELD_PER_PEER * 67 + 70)).unwrap();
    assert!(
        written + removals + held < crate::fleet::store::MAX_PEERS,
        "{written} + {removals} + {held}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_removal_dropped_for_room_keeps_its_target_out() {
    let (a, b, c, d) = four().await;
    // a's removal of c, on an operation b has not seen.
    let missing = unseen(&a, 0);
    let kept = removal(&a, &c, None);
    b.fleet.receive(vec![kept], now_ms()).unwrap();
    // More of a's removals of d than the pool holds, each on its own
    // unseen parent, arrive later: the first, c's, is dropped for room.
    let mut flood = Vec::new();
    for turn in 1..=crate::control::fleet::MAX_PENDING + 4 {
        unseen(&a, turn);
        flood.push(removal(&a, &d, None));
    }
    b.fleet.receive(flood, now_ms() + 1).unwrap();
    // Its history arrives: nothing refused it, so c is never let back in.
    b.fleet.receive(vec![missing], now_ms() + 2).unwrap();
    let out = {
        let inner = b.fleet.inner();
        !inner.log.as_ref().unwrap().is_member(&c.fleet.me())
    };
    assert!(out || b.fleet.suspended().contains(&c.fleet.me()));
}

#[test]
fn only_a_removals_own_fault_counts_as_refused() {
    use crate::control::fleet::{Error as LogError, Inserted};
    for room in [LogError::Full, LogError::Bounds, LogError::ParentMissing] {
        assert!(!suspensions::refuses(&Err(room)));
    }
    for fault in [
        LogError::Sequence,
        LogError::Signature,
        LogError::Permission,
        LogError::Unknown,
    ] {
        assert!(suspensions::refuses(&Err(fault)));
    }
    assert!(!suspensions::refuses(&Ok(Inserted::Applied)));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_folded_removal_its_history_refuses_lifts_its_target() {
    let (a, b, c, d) = four().await;
    // a's removal of c on an operation b has not seen, reusing that
    // operation's number: its history refuses it.
    let missing = unseen(&a, 0);
    let invalid = removal(&a, &c, Some(missing.unsigned().seq));
    b.fleet.receive(vec![invalid.clone()], now_ms()).unwrap();
    // Enough of a's removals of d after it to fold it, by id.
    for turn in 1..=MAX_UNCONFIRMED_PER_SIGNER {
        unseen(&a, turn);
        b.fleet
            .receive(vec![removal(&a, &d, None)], now_ms() + 1)
            .unwrap();
    }
    assert!(
        b.fleet.inner().folded[&a.fleet.me()][&c.fleet.me()]
            .ids
            .contains_key(&invalid.id())
    );
    // Its history arrives: refused, and c is let back in; d stays out.
    b.fleet.receive(vec![missing], now_ms() + 2).unwrap();
    assert_eq!(b.fleet.suspended(), vec![d.fleet.me()]);
}

#[tokio::test(flavor = "multi_thread")]
async fn an_overflowed_target_ends_at_its_time() {
    let (a, b, c, d) = four().await;
    let start = now_ms();
    let (signer, target, other) = (a.fleet.me(), c.fleet.me(), d.fleet.me());
    let mut inner = b.fleet.inner();
    // More removals of c fold than are kept by id, the last a day on;
    // the newest, of d, stay apart.
    kept(&mut inner, signer, &[target], start, MAX_FOLDED_IDS + 2, 1);
    kept(&mut inner, signer, &[target], start + DAY, 1, 50);
    kept(
        &mut inner,
        signer,
        &[other],
        start + 2 * DAY,
        MAX_UNCONFIRMED_PER_SIGNER,
        100,
    );
    suspensions::settle(&mut inner, start + 2 * DAY, BTreeSet::new());
    let folded = &inner.folded[&signer][&target];
    assert_eq!(folded.ids.len(), MAX_FOLDED_IDS);
    assert_eq!(folded.overflow.as_ref().unwrap().since_ms, start + DAY);
    // The ids end a week after the start; the overflow holds c until a
    // week after its latest, then it too ends.
    suspensions::settle(&mut inner, start + 7 * DAY + 1, BTreeSet::new());
    assert!(inner.folded[&signer][&target].ids.is_empty());
    assert!(inner.folded[&signer].contains_key(&target));
    suspensions::settle(&mut inner, start + 8 * DAY + 1, BTreeSet::new());
    assert!(
        inner
            .folded
            .get(&signer)
            .is_none_or(|folded| !folded.contains_key(&target))
    );
}

#[test]
fn a_saves_merge_keeps_the_newest_ids_and_folds_the_rest() {
    let (signer, target) = (MemberId([1; 32]), MemberId([2; 32]));
    let folded = |first: u8, since_ms: u64| Suspensions {
        folded: BTreeMap::from([(
            signer,
            BTreeMap::from([(
                target,
                FoldedTarget {
                    ids: (0..MAX_FOLDED_IDS)
                        .map(|turn| {
                            let turn = u8::try_from(turn).unwrap();
                            let removal = FoldedRemoval {
                                since_ms: since_ms + u64::from(turn),
                                named: BTreeSet::from([target, MemberId([first; 32])]),
                                pinned: false,
                            };
                            (OpId([first + turn; 32]), removal)
                        })
                        .collect(),
                    overflow: None,
                },
            )]),
        )]),
        ..Suspensions::default()
    };
    // Held now: newer; on disk: older, sharing no id.
    let merged = crate::fleet::store::merged(&folded(10, 2_000), &folded(50, 1_000));
    let kept = &merged.folded[&signer][&target];
    assert_eq!(kept.ids.len(), MAX_FOLDED_IDS);
    assert!(kept.ids.values().all(|removal| removal.since_ms >= 2_000));
    let overflow = kept.overflow.as_ref().unwrap();
    let latest = 1_000 + u64::try_from(MAX_FOLDED_IDS).unwrap() - 1;
    assert_eq!(overflow.since_ms, latest);
    assert_eq!(overflow.named[&MemberId([50; 32])], latest);
}
