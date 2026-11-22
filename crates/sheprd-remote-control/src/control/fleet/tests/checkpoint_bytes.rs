//! A checkpoint names one encoding of its state, key versions included,
//! and a log pruned to one saves and loads again.

use super::*;

#[test]
fn a_log_pruned_to_a_checkpoint_saves_and_loads() {
    let (a, b) = (computer(1), computer(2));
    let mut log = found(&a);
    admit(&mut log, &a, &b);
    let mut bundle = b.bundle.clone();
    bundle.tunnel = [0x62; 32];
    act(&mut log, &b, Body::Rotate(bundle));
    let hash = log.state().hash().unwrap();
    let checkpoint = act(&mut log, &a, Body::Checkpoint(hash));
    log.prune(&checkpoint.id()).unwrap();
    let loaded = saved::decode(&saved::encode(&log).unwrap(), NOW).unwrap();
    assert_eq!(loaded.state(), log.state());
    assert_ne!(loaded.state().members[&b.id()].rotated, 0);
}

#[test]
fn a_checkpoint_naming_the_state_without_its_key_versions_is_refused() {
    let (a, b) = (computer(1), computer(2));
    let mut log = found(&a);
    admit(&mut log, &a, &b);
    let mut bundle = b.bundle.clone();
    bundle.tunnel = [0x62; 32];
    act(&mut log, &b, Body::Rotate(bundle));
    let mut stripped = log.state().clone();
    for member in stripped.members.values_mut() {
        member.rotated = 0;
    }
    let op = sign(&log, &a, Body::Checkpoint(stripped.hash().unwrap()));
    assert_eq!(log.insert(op, NOW), Err(Error::Checkpoint));
    // Nor does a joiner start from those bytes.
    let op = sign(&log, &a, Body::Checkpoint(stripped.hash().unwrap()));
    let whole = log.state().encode().unwrap();
    assert_eq!(
        Log::from_checkpoint(op, &whole).err(),
        Some(Error::Checkpoint)
    );
}
