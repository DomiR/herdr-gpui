//! A log pruned to a checkpoint saves a file that loads again, with the
//! same state: the file keeps the state the checkpoint names.

use super::*;

#[test]
fn a_log_pruned_to_a_checkpoint_saves_and_loads() {
    let (a, b) = (computer(1), computer(2));
    let mut log = found(&a);
    admit(&mut log, &a, &b);
    let hash = log.state().hash().unwrap();
    let checkpoint = act(&mut log, &a, Body::Checkpoint(hash));
    log.prune(&checkpoint.id()).unwrap();
    let loaded = saved::decode(&saved::encode(&log).unwrap(), NOW).unwrap();
    assert_eq!(loaded.state(), log.state());
    assert_eq!(loaded.heads(), log.heads());
}
