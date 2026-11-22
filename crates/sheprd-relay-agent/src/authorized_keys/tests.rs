#![allow(clippy::unwrap_used)]

use super::*;

pub(crate) fn key(seed: u8) -> String {
    let blob = [
        &11u32.to_be_bytes()[..],
        b"ssh-ed25519",
        &32u32.to_be_bytes(),
        &[seed; 32],
    ]
    .concat();
    format!("ssh-ed25519 {}", STANDARD.encode(blob))
}

fn member(id: &str, seed: u8) -> (String, String) {
    (id.to_owned(), key(seed))
}

#[test]
fn only_an_ed25519_key_is_taken_and_its_comment_dropped() {
    assert_eq!(ed25519(&format!("{} me@desk", key(1))), Some(key(1)));
    assert_eq!(ed25519("ssh-rsa AAAAB3NzaC1yc2E="), None);
    assert_eq!(ed25519("ssh-ed25519 AAAA"), None);
    assert_eq!(ed25519(&"x".repeat(MAX_KEY + 1)), None);
}

#[test]
fn the_fleet_lines_are_restricted_and_tagged() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("authorized_keys");
    set_fleet(&path, "aaaa", &[member("m1", 1)]).unwrap();
    let text = fs::read_to_string(&path).unwrap();
    assert_eq!(
        text,
        format!(
            "from=\"127.0.0.1,::1\",no-agent-forwarding,no-X11-forwarding {} sheprd-fleet:aaaa:m1\n",
            key(1)
        )
    );
}

#[test]
fn the_users_lines_and_other_fleets_lines_stay() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("authorized_keys");
    let other = fleet_line("bbbb:m9", &key(9));
    let mine = format!("{} me@laptop sheprd-fleet:aaaa:m1", key(5));
    fs::write(&path, format!("{mine}\n{other}\n")).unwrap();
    set_fleet(&path, "aaaa", &[member("m1", 1), member("m2", 2)]).unwrap();
    set_fleet(&path, "aaaa", &[member("m2", 2)]).unwrap();
    let text = fs::read_to_string(&path).unwrap();
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines[..2], [mine.as_str(), other.as_str()]);
    assert_eq!(lines[2], fleet_line("aaaa:m2", &key(2)));
    assert_eq!(lines.len(), 3);
}

#[test]
fn a_missing_file_with_nothing_to_add_stays_missing() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("authorized_keys");
    set_fleet(&path, "aaaa", &[]).unwrap();
    assert!(!path.exists());
}

#[cfg(unix)]
#[test]
fn a_linked_file_is_refused_and_left_alone() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("elsewhere");
    fs::write(&target, "keep\n").unwrap();
    let path = dir.path().join("authorized_keys");
    std::os::unix::fs::symlink(&target, &path).unwrap();
    assert!(matches!(
        set_fleet(&path, "aaaa", &[member("m1", 1)]),
        Err(Error::Linked { .. })
    ));
    assert_eq!(fs::read_to_string(&target).unwrap(), "keep\n");
}

#[test]
fn a_file_too_large_is_not_rewritten() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("authorized_keys");
    fs::write(&path, vec![b'#'; usize::try_from(MAX_FILE).unwrap() + 1]).unwrap();
    assert!(matches!(
        set_fleet(&path, "aaaa", &[member("m1", 1)]),
        Err(Error::TooLarge { .. })
    ));
}

#[cfg(unix)]
#[test]
fn the_rewritten_file_is_private() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("authorized_keys");
    fs::write(&path, "# mine\n").unwrap();
    set_fleet(&path, "aaaa", &[member("m1", 1)]).unwrap();
    let mode = fs::metadata(&path).unwrap().permissions().mode();
    assert_eq!(mode & 0o777, 0o600);
}

#[test]
fn a_users_line_ending_in_the_marker_is_not_the_fleets() {
    assert_eq!(fleet_tag(&format!("{} sheprd-fleet:aaaa:m1", key(1))), None);
    assert_eq!(fleet_tag(&fleet_line("aaaa:m1", &key(1))), Some("aaaa:m1"));
}

#[test]
fn lines_in_another_encoding_are_kept_byte_for_byte() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("authorized_keys");
    // Latin-1 in a comment and in a forced command, and a CRLF line.
    let user =
        b"command=\"echo caf\xe9\" ssh-ed25519 AAAA ren\xe9@desk\nssh-rsa BBBB me\r\n".to_vec();
    fs::write(&path, &user).unwrap();
    set_fleet(&path, "aaaa", &[member("m1", 1)]).unwrap();
    let written = fs::read(&path).unwrap();
    assert!(written.starts_with(&user), "{written:?}");
    set_fleet(&path, "aaaa", &[]).unwrap();
    assert_eq!(fs::read(&path).unwrap(), user);
}
