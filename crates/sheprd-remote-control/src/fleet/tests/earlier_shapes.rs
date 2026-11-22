//! Folded suspensions saved by earlier builds read back failing closed:
//! the target stays out until its time, never let in by a field the
//! reader does not know.

use super::*;

/// b's fleet, reloaded from `peers.json` holding `peers`, with a's
/// suspension of c in it.
async fn reloaded(peers: impl FnOnce(&str, &str, u64) -> String) -> (Node, Node, Fleet) {
    let (a, b, c) = idle().await;
    let now = now_ms();
    std::fs::write(
        b.dir.path().join("fleet/peers.json"),
        peers(&a.me(), &c.me(), now),
    )
    .unwrap();
    let again = Fleet::load(b.dir.path(), now).unwrap();
    (a, c, again)
}

#[tokio::test(flavor = "multi_thread")]
async fn a_folded_target_with_its_names_reads_back() {
    // As written with the latest arrival and the names directly.
    let (_, c, again) = reloaded(|signer, target, now| {
        format!(
            r#"{{"folds": {{"{signer}": {{"{target}": {{"since_ms": {now}, "named": {{"{target}": {now}}}}}}}}}}}"#
        )
    })
    .await;
    assert_eq!(again.suspended(), vec![c.fleet.me()]);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_folded_target_with_ids_by_arrival_reads_back() {
    // As written with each id's arrival, an overflow by time, and a flag.
    let id = "ab".repeat(32);
    let (_, c, again) = reloaded(|signer, target, now| {
        format!(
            r#"{{"folds": {{"{signer}": {{"{target}": {{"ids": {{"{id}": {now}}}, "overflowed": {now}, "named_with": ["{target}"], "pinned": false}}}}}}}}"#
        )
    })
    .await;
    assert_eq!(again.suspended(), vec![c.fleet.me()]);
    let inner = again.inner();
    let folded = inner
        .folded
        .values()
        .next()
        .unwrap()
        .values()
        .next()
        .unwrap();
    assert!(folded.ids.values().all(|removal| removal.pinned));
    assert!(folded.overflow.is_some());
}

#[tokio::test(flavor = "multi_thread")]
async fn the_first_folded_shape_reads_back() {
    let id = "cd".repeat(32);
    let (_, c, again) = reloaded(|signer, target, now| {
        format!(
            r#"{{"folded": [{{"signer": "{signer}", "target": "{target}", "ids": {{"{id}": {now}}}}}]}}"#
        )
    })
    .await;
    assert_eq!(again.suspended(), vec![c.fleet.me()]);
}

#[tokio::test(flavor = "multi_thread")]
async fn this_builds_shape_reads_back_unchanged() {
    let (a, b, c) = idle().await;
    let id = OpId([7; 32]);
    {
        let mut inner = b.fleet.inner();
        inner.folded.insert(
            a.fleet.me(),
            BTreeMap::from([(
                c.fleet.me(),
                suspensions::FoldedTarget {
                    ids: BTreeMap::from([(
                        id,
                        suspensions::FoldedRemoval {
                            since_ms: now_ms(),
                            named: BTreeSet::from([c.fleet.me()]),
                            pinned: false,
                        },
                    )]),
                    overflow: None,
                },
            )]),
        );
        b.fleet.save(&inner).unwrap();
    }
    let again = Fleet::load(b.dir.path(), now_ms()).unwrap();
    assert_eq!(again.inner().folded, b.fleet.inner().folded);
}
