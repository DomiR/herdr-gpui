use super::*;
use std::fs;

#[test]
fn reads_and_writes_atomically_keeping_permissions() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("todo.md");
    fs::write(&path, "a\n").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o640)).unwrap();
    }
    let (text, stamp) = disk::read(&path).unwrap();
    assert_eq!(text, "a\n");
    let written = disk::write_if_unchanged(&path, "a\n", "b\n").unwrap();
    let disk::Written::Wrote(after) = written else {
        panic!("expected a write, got {written:?}");
    };
    assert_ne!(after, stamp);
    assert_eq!(fs::read_to_string(&path).unwrap(), "b\n");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o640);
    }
    // No temporary file is left behind.
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
}

#[test]
fn a_write_does_not_overwrite_a_change_it_has_not_seen() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("todo.md");
    fs::write(&path, "agent\n").unwrap();
    let written = disk::write_if_unchanged(&path, "synced\n", "mine\n").unwrap();
    assert_eq!(written, disk::Written::Changed("agent\n".into()));
    assert_eq!(fs::read_to_string(&path).unwrap(), "agent\n");
}

#[test]
fn a_deleted_or_moved_file_is_not_brought_back() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("todo.md");
    fs::write(&path, "a\n").unwrap();
    fs::rename(&path, dir.path().join("moved.md")).unwrap();
    assert!(matches!(disk::read(&path), Err(Error::Missing { .. })));
    assert_eq!(disk::stamp(&path).unwrap(), None);
    assert_eq!(
        disk::write_if_unchanged(&path, "a\n", "b\n").unwrap(),
        disk::Written::Missing
    );
    assert!(!path.exists());
}

#[test]
fn refuses_what_is_not_a_bounded_utf8_file() {
    let dir = tempfile::tempdir().unwrap();
    let binary = dir.path().join("binary.md");
    fs::write(&binary, [0xff, 0xfe, 0x00]).unwrap();
    assert!(matches!(disk::read(&binary), Err(Error::NotUtf8 { .. })));
    let huge = dir.path().join("huge.md");
    fs::write(&huge, vec![b'x'; MAX_BYTES + 1]).unwrap();
    assert!(matches!(disk::read(&huge), Err(Error::TooLarge { .. })));
    assert!(matches!(
        disk::read(dir.path()),
        Err(Error::NotAFile { .. })
    ));
    assert!(matches!(
        disk::write(&huge, &"x".repeat(MAX_BYTES + 1)),
        Err(Error::TooLarge { .. })
    ));
}

#[test]
fn the_stamp_changes_with_the_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("todo.md");
    fs::write(&path, "a\n").unwrap();
    let first = disk::stamp(&path).unwrap();
    disk::write(&path, "a longer text\n").unwrap();
    assert_ne!(disk::stamp(&path).unwrap(), first);
}

#[test]
fn recreating_brings_back_a_gone_file_but_not_over_a_new_one() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("todo.md");
    assert!(matches!(
        disk::recreate(&path, "a\n", "b\n").unwrap(),
        disk::Written::Wrote(_)
    ));
    assert_eq!(fs::read_to_string(&path).unwrap(), "b\n");
    fs::write(&path, "tool\n").unwrap();
    assert_eq!(
        disk::recreate(&path, "b\n", "mine\n").unwrap(),
        disk::Written::Changed("tool\n".into())
    );
}
