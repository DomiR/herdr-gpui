//! Removing a compromised member that admitted more members than one
//! removal names: none is left out.
use super::*;
use crate::settings_window::pages::fleet::removal;
use sheprd_remote_control::control::fleet::{
    HostKind, HostView, Kind, MAX_ALSO_MEMBERS, MemberView, Reason, View, sync::Ask,
};

fn view(admitted: usize) -> View {
    let member = |id: String, by: Option<&str>| MemberView {
        name: id.clone(),
        id,
        kind: Kind::Computer,
        admitted_by: by.map(str::to_owned),
    };
    let mut members = vec![member("me".into(), None), member("bad".into(), Some("me"))];
    members.extend((0..admitted).map(|index| member(format!("m{index:02}"), Some("bad"))));
    View {
        fleet: "f".repeat(64),
        me: "me".into(),
        members,
        hosts: vec![HostView {
            id: "hx".into(),
            name: "build-01".into(),
            kind: HostKind::Sheprd,
            user: "me".into(),
            added_by: "bad".into(),
            grants: Vec::new(),
            relays: Vec::new(),
            target: None,
            rev: 1,
            held: false,
            flagged: false,
        }],
        removed: 0,
        equivocating: Vec::new(),
        suspended: Vec::new(),
        pending: 0,
        heads: Vec::new(),
        hosted: None,
    }
}

/// Every member the removals name, targets first, and every host.
fn named(asks: &[Ask]) -> (Vec<String>, Vec<String>) {
    let (mut members, mut hosts) = (Vec::new(), Vec::new());
    for ask in asks {
        let Ask::Remove {
            member,
            also_members,
            also_hosts,
            reason,
        } = ask
        else {
            panic!("not a removal: {ask:?}");
        };
        assert_eq!(*reason, Reason::Compromised);
        assert!(also_members.len() <= MAX_ALSO_MEMBERS);
        members.push(member.clone());
        members.extend(also_members.iter().cloned());
        hosts.extend(also_hosts.iter().cloned());
    }
    (members, hosts)
}

#[test]
fn a_compromised_member_takes_every_member_it_admitted_in_bounded_removals() {
    let view = view(40);
    let asks = removal(&view, "bad", Reason::Compromised);
    assert!(asks.len() > 1, "{asks:?}");
    let (members, hosts) = named(&asks);
    let mut expected = vec!["bad".to_owned()];
    expected.extend((0..40).map(|index| format!("m{index:02}")));
    assert_eq!(members, expected);
    assert_eq!(hosts, vec!["hx".to_owned()]);
}

#[test]
fn a_compromised_member_with_few_admitted_is_one_removal() {
    let view = view(MAX_ALSO_MEMBERS);
    let asks = removal(&view, "bad", Reason::Compromised);
    assert_eq!(asks.len(), 1);
    assert_eq!(named(&asks).0.len(), MAX_ALSO_MEMBERS + 1);
    // Any other reason takes nothing with it.
    assert_eq!(
        removal(&view, "bad", Reason::Lost),
        vec![Ask::Remove {
            member: "bad".into(),
            also_members: Vec::new(),
            also_hosts: Vec::new(),
            reason: Reason::Lost,
        }]
    );
}
