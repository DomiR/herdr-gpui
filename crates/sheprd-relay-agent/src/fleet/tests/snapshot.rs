//! The running agent reads who it admits as one snapshot: never part
//! way through an update, which holds the state directory's lock.

use super::*;
use std::{sync::mpsc, time::Duration};

#[test]
fn a_reload_waits_for_an_update_to_finish_and_reads_all_of_it() {
    let host = Host::new();
    host.apply(file(
        &[(1, true, 0), (2, false, 0), (3, false, 0)],
        &[],
        &[],
    ))
    .unwrap();
    // An update under way: the lock held, its first files written.
    let lock = files::Lock::acquire(host.state()).unwrap();
    let mut next = Held::load(host.state()).unwrap();
    next.crowded.insert(id(1));
    next.save(host.state(), &BTreeSet::new()).unwrap();
    let state = host.state().to_owned();
    let (done, read) = mpsc::channel();
    let reader = std::thread::spawn(move || {
        done.send(Admitted::load(&state).unwrap()).unwrap();
    });
    // The reload does not read while the update holds the lock.
    assert!(read.recv_timeout(Duration::from_millis(300)).is_err());
    // The update finishes: member 3 is removed and no one waits.
    next.crowded.clear();
    next.access = Some(file(&[(1, true, 0), (2, false, 0)], &[3], &[]));
    next.save(host.state(), &[id(1)].into_iter().collect())
        .unwrap();
    drop(lock);
    let admitted = read.recv_timeout(Duration::from_secs(10)).unwrap();
    reader.join().unwrap();
    assert!(admitted.held().crowded.is_empty());
    assert!(
        admitted
            .held()
            .access
            .as_ref()
            .unwrap()
            .removed
            .contains(&id(3))
    );
    assert_eq!(admitted.peer(&dial(3), &[3; 32]), None);
}
