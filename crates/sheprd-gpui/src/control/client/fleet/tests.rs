use super::*;
use sheprd_remote_control::control::fleet::{HostView, MemberView};

fn view() -> View {
    View {
        fleet: "ab".repeat(32),
        me: "a1".into(),
        members: vec![
            MemberView {
                id: "a1".into(),
                name: "studio".into(),
                kind: Kind::Computer,
                admitted_by: None,
            },
            MemberView {
                id: "p2".into(),
                name: "iPhone".into(),
                kind: Kind::Phone,
                admitted_by: Some("a1".into()),
            },
        ],
        hosts: vec![HostView {
            id: "h1".into(),
            name: "build-01".into(),
            kind: HostKind::Agent,
            user: "deploy".into(),
            added_by: "a1".into(),
            grants: vec!["a1".into()],
            relays: Vec::new(),
            target: Some("deploy@r-0011223344556677.sheprd.invalid".into()),
            rev: 1,
            held: false,
            flagged: false,
        }],
        removed: 1,
        equivocating: Vec::new(),
        suspended: Vec::new(),
        pending: 0,
        heads: Vec::new(),
        hosted: None,
    }
}

#[test]
fn status_names_members_and_hosts() {
    let view = view();
    let all = text(&view, Part::All);
    assert!(all.contains("2 members, 1 hosts, 1 removed"), "{all}");
    assert!(
        all.contains("a1  studio  computer  founder  (this computer)"),
        "{all}"
    );
    assert!(
        all.contains("p2  iPhone  phone  admitted by studio"),
        "{all}"
    );
    assert!(all.contains("h1  build-01  agent  added by studio  1 allowed  ssh deploy@r-0011223344556677.sheprd.invalid"), "{all}");
    assert!(!text(&view, Part::Members).contains("build-01"));
    assert!(!text(&view, Part::Hosts).contains("iPhone"));
}

#[test]
fn no_fleet_prints_nothing_found() {
    let output = output(
        &Reply::Status {
            view: None,
            error: None,
        },
        Part::All,
    )
    .unwrap();
    assert_eq!(output.text, "This computer is in no fleet");
    assert_eq!(output.status, crate::control::client::exit::NOTHING);
}

#[test]
fn a_daemon_that_cannot_be_read_fails_rather_than_saying_no_fleet() {
    let output = output(
        &Reply::Status {
            view: None,
            error: Some("Connection refused".into()),
        },
        Part::All,
    )
    .unwrap();
    assert_eq!(output.text, "Could not read the fleet: Connection refused");
    assert_eq!(output.status, crate::control::client::exit::REFUSED);
}
