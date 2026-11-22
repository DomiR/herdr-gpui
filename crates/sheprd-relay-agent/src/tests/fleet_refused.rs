//! A running agent whose fleet files stop reading shuts the fleet out: a
//! refusal never depends on reading what it refuses.

use super::fleet::{FLEET, apply, closes, lines, member, open};
use super::*;
use crate::{
    access::Id,
    fleet::{Held, REFUSED, SUSPENDED},
    removal::PURPOSE_SSH,
};

#[tokio::test(flavor = "multi_thread")]
async fn a_refusal_over_a_held_file_that_no_longer_reads_shuts_the_fleet_out() {
    let dir = tempfile::tempdir().unwrap();
    let running = start(dir.path()).await;
    let a = Desktop::new(0x51);
    apply(&running, dir.path(), vec![member(&a, 1, true, 0)], &[], &[]);
    assert_eq!(lines(dir.path()), [Id([1; 32]).short()]);
    let mut live = open(&running, &a, PURPOSE_SSH).await;
    greeting(&mut live.io).await;

    // The held file is damaged on disk, then a bad file is handed over.
    let stored = dir.path().join(crate::access::FILE);
    let mut bytes = std::fs::read(&stored).unwrap();
    bytes.truncate(bytes.len() / 2);
    std::fs::write(&stored, &bytes).unwrap();
    let keys = dir.path().join("authorized_keys");
    assert!(Held::apply_bytes(dir.path(), Some(&keys), b"nope").is_err());
    // The lines go though the held file names no fleet any more.
    assert!(lines(dir.path()).is_empty());
    // The agent reads its files again, and shuts the member out.
    let held = Held::load(dir.path()).unwrap();
    assert!(held.refused && held.access.is_none());
    running.agent.reload().unwrap();
    assert!(closes(&mut live.io).await, "the member's session lived on");
}

#[tokio::test(flavor = "multi_thread")]
async fn fleet_files_that_stop_reading_shut_the_running_agents_fleet_out() {
    let dir = tempfile::tempdir().unwrap();
    let running = start(dir.path()).await;
    let a = Desktop::new(0x51);
    apply(&running, dir.path(), vec![member(&a, 1, true, 0)], &[], &[]);
    let mut live = open(&running, &a, PURPOSE_SSH).await;
    greeting(&mut live.io).await;
    // Neither a refusal nor a good file can make this one read.
    std::fs::write(dir.path().join(SUSPENDED), b"{").unwrap();
    assert!(running.agent.reload().is_err());
    assert!(closes(&mut live.io).await, "the member's session lived on");
    // `fleet apply` refuses too, and marks it.
    let keys = dir.path().join("authorized_keys");
    let good = crate::access::Access::new(Id(FLEET));
    assert!(Held::apply(dir.path(), Some(&keys), good).is_err());
    assert!(dir.path().join(REFUSED).exists());
    assert!(lines(dir.path()).is_empty());
}
