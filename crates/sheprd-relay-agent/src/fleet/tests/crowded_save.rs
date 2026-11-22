//! A member waiting for room stays waiting until the file that frees the
//! room is kept: a save that stops part way never lets it take a slot.

use super::*;

#[test]
fn a_lifted_wait_is_saved_only_after_the_file_that_frees_the_room() {
    let host = Host::new();
    host.apply(file(
        &[(1, true, 0), (2, false, 0), (3, false, 0)],
        &[],
        &[],
    ))
    .unwrap();
    let mut held = Held::load(host.state()).unwrap();
    held.crowded.insert(id(1));
    held.save(host.state(), &held.crowded).unwrap();
    // The next file removes member 3, and no one waits any more.
    let mut next = held.clone();
    next.crowded.clear();
    next.access = Some(file(&[(1, true, 0), (2, false, 0)], &[3], &[]));
    let files = next.files(&held.crowded).unwrap();
    let names: Vec<&str> = files.iter().map(|(name, _)| *name).collect();
    assert_eq!(names, [REVOKED, CROWDED, access::FILE, CROWDED, SUSPENDED]);
    // Stopped before the access file: member 1 still waits.
    for (name, bytes) in &files[..2] {
        files::write_private(&host.state().join(name), bytes).unwrap();
    }
    let stopped = Held::load(host.state()).unwrap();
    assert!(stopped.crowded.contains(&id(1)));
    assert!(!stopped.reaches_ssh(&id(1)));
    // Every file written: it no longer waits.
    for (name, bytes) in &files {
        files::write_private(&host.state().join(name), bytes).unwrap();
    }
    assert!(Held::load(host.state()).unwrap().reaches_ssh(&id(1)));
}

#[test]
fn a_new_wait_is_saved_before_the_file_that_needs_it() {
    let held = Held {
        access: Some(file(&[(1, true, 0)], &[], &[])),
        crowded: [id(1)].into_iter().collect(),
        ..Held::default()
    };
    let files = held.files(&BTreeSet::new()).unwrap();
    let names: Vec<&str> = files.iter().map(|(name, _)| *name).collect();
    assert_eq!(names, [REVOKED, CROWDED, access::FILE, SUSPENDED]);
}
