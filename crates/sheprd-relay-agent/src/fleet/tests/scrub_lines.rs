//! A scrub or a refusal removes every `authorized_keys` line the agent
//! wrote, even when the access file that named them was never saved.

use super::*;

const USER: &str = "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIBBBB me@desk\n";

/// A host whose fleet lines were written, then the access file lost, as
/// an apply stopped before saving it leaves things.
fn lines_without_a_file() -> Host {
    let host = Host::new();
    std::fs::write(host.keys(), USER).unwrap();
    host.apply(file(&[(1, true, 0), (2, true, 0)], &[], &[]))
        .unwrap();
    assert_eq!(host.lines().len(), 2);
    std::fs::remove_file(host.state().join(access::FILE)).unwrap();
    host
}

fn user_line_kept(host: &Host) {
    let text = std::fs::read_to_string(host.keys()).unwrap();
    assert!(text.contains(USER.trim_end()), "{text}");
}

#[test]
fn a_scrub_removes_lines_whose_access_file_was_never_saved() {
    let host = lines_without_a_file();
    Held::scrub(host.state(), Some(&host.keys())).unwrap();
    assert!(host.lines().is_empty());
    user_line_kept(&host);
    assert!(!host.state().join(LINES).exists());
}

#[test]
fn a_refusal_removes_lines_whose_access_file_was_never_saved() {
    let host = lines_without_a_file();
    assert!(Held::apply_bytes(host.state(), Some(&host.keys()), b"nope").is_err());
    assert!(host.lines().is_empty());
    user_line_kept(&host);
}

#[test]
fn the_record_of_the_lines_comes_before_them() {
    let host = Host::new();
    host.apply(file(&[(1, true, 0)], &[], &[])).unwrap();
    let recorded = std::fs::read_to_string(host.state().join(LINES)).unwrap();
    assert_eq!(recorded, id(FLEET).short());
}

#[test]
fn another_fleets_lines_stay_when_this_agent_wrote_none() {
    let host = Host::new();
    let other = authorized_keys::fleet_line(
        "0123456789abcdef:fedcba9876543210",
        &authorized_keys::tests::key(9),
    );
    std::fs::write(host.keys(), format!("{USER}{other}\n")).unwrap();
    Held::scrub(host.state(), Some(&host.keys())).unwrap();
    assert!(Held::apply_bytes(host.state(), Some(&host.keys()), b"nope").is_err());
    let text = std::fs::read_to_string(host.keys()).unwrap();
    assert!(text.contains(&other), "{text}");
}

#[test]
fn a_record_that_does_not_read_takes_every_fleet_line() {
    let host = lines_without_a_file();
    std::fs::write(host.state().join(LINES), b"\xff not a fleet").unwrap();
    Held::scrub(host.state(), Some(&host.keys())).unwrap();
    assert!(host.lines().is_empty());
    user_line_kept(&host);
}
