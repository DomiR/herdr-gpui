//! Plain hosts are shared and kept current only over sessions that check
//! the host key strictly.

use super::*;

fn ssh(target: &str) -> ConnectTarget {
    ConnectTarget::Ssh {
        target: target.into(),
        session: "default".into(),
    }
}

#[test]
fn a_machine_whose_key_is_not_checked_is_refused() {
    let target = "deploy@unchecked-plain.example";
    sheprd_client::set_host_keys(target, sheprd_client::HostKeys::Skip);
    let refused = crate::fleet::plain::strict_target(&ssh(target));
    sheprd_client::set_host_keys(target, sheprd_client::HostKeys::Verify);
    assert!(matches!(refused, Err(Error::Unchecked)));
    assert_eq!(
        crate::fleet::plain::strict_target(&ssh(target)).unwrap(),
        target
    );
}

#[test]
fn only_an_ssh_machine_can_be_a_plain_host() {
    assert!(matches!(
        crate::fleet::plain::strict_target(&ConnectTarget::Local),
        Err(Error::Unsupported)
    ));
}

#[test]
fn a_machine_whose_key_is_not_checked_is_asked_rarely() {
    let mine = vec!["h1".to_owned()];
    let now = Instant::now() + Duration::from_secs(7200);
    let link = Link {
        standing: Standing::Unchecked,
        last: Some(now - Duration::from_secs(600)),
        ..Link::default()
    };
    assert!(!link.due(&mine, now));
}

fn held(fleet: [u8; 32]) -> sheprd_relay_agent::fleet::Report {
    sheprd_relay_agent::fleet::Report {
        allowed: 1,
        crowded: Default::default(),
        denied: 0,
        digest: String::new(),
        fleet: sheprd_relay_agent::access::Id(fleet),
        refused: false,
        removed: 0,
        revoked: Vec::new(),
        suspended: Default::default(),
    }
}

#[test]
fn an_agent_whose_fleet_access_was_removed_is_not_brought_back_by_a_sync() {
    let ours = "ab".repeat(32);
    // `fleet scrub` left the agent in no fleet: only a person shares it.
    assert_eq!(
        crate::fleet::plain::not_ours(None, &ours),
        Some(Standing::NoDaemon)
    );
    assert_eq!(
        crate::fleet::plain::not_ours(Some(&held([0xcd; 32])), &ours),
        Some(Standing::Outside)
    );
    assert_eq!(
        crate::fleet::plain::not_ours(Some(&held([0xab; 32])), &ours),
        None
    );
}

#[test]
fn an_agent_of_a_host_removed_everywhere_is_told_to_forget_the_fleet() {
    use crate::fleet::plain::{Next, next};
    use sheprd_remote_control::control::fleet::{HostKind, HostView, View};
    let host = |id: &str| HostView {
        id: id.into(),
        name: "build-01".into(),
        kind: HostKind::Agent,
        user: "deploy".into(),
        added_by: "a".into(),
        grants: Vec::new(),
        relays: Vec::new(),
        target: None,
        rev: 1,
        held: false,
        flagged: false,
    };
    let view = View {
        fleet: "f".repeat(64),
        me: "a".into(),
        members: Vec::new(),
        hosts: vec![host("live")],
        removed: 0,
        equivocating: Vec::new(),
        suspended: Vec::new(),
        pending: 0,
        heads: Vec::new(),
        hosted: None,
    };
    assert_eq!(next(&view, "live", &[]), Next::Keep);
    assert_eq!(next(&view, "gone", &["gone".to_owned()]), Next::Forget);
    assert_eq!(next(&view, "other", &["gone".to_owned()]), Next::Leave);
}

#[test]
fn an_agent_with_members_waiting_for_room_is_handed_the_file_again() {
    let file = sheprd_relay_agent::access::Access::new(sheprd_relay_agent::access::Id([1; 32]));
    let digest = file.digest().unwrap();
    let mut report = held([1; 32]);
    report.digest.clone_from(&digest);
    assert!(crate::fleet::plain::current(Some(&report), &digest, &file));
    report
        .crowded
        .insert(sheprd_relay_agent::access::Id([2; 32]));
    assert!(!crate::fleet::plain::current(Some(&report), &digest, &file));
    assert!(!crate::fleet::plain::current(None, &digest, &file));
}

#[test]
fn an_agent_that_refused_its_last_file_is_handed_a_good_one_again() {
    let file = sheprd_relay_agent::access::Access::new(sheprd_relay_agent::access::Id([1; 32]));
    let digest = file.digest().unwrap();
    let mut report = held([1; 32]);
    report.digest.clone_from(&digest);
    report.refused = true;
    assert!(!crate::fleet::plain::current(Some(&report), &digest, &file));
}
