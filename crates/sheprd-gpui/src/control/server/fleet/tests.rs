use super::*;
use sheprd_remote_control::control::fleet::{HostKind, HostView, Kind, MemberView};

fn view() -> View {
    let member = |id: &str, name: &str| MemberView {
        id: id.into(),
        name: name.into(),
        kind: Kind::Computer,
        admitted_by: None,
    };
    View {
        fleet: "f".into(),
        me: "a1".into(),
        members: vec![
            member("a1", "studio"),
            member("b2", "laptop"),
            member("c3", "laptop"),
        ],
        hosts: vec![HostView {
            id: "h1".into(),
            name: "build-01".into(),
            kind: HostKind::Agent,
            user: "deploy".into(),
            added_by: "a1".into(),
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

#[test]
fn members_and_hosts_are_named_by_id_or_by_a_unique_name() {
    let view = view();
    assert_eq!(pick_member(Some(&view), "studio").unwrap(), "a1");
    assert_eq!(pick_member(Some(&view), "c3").unwrap(), "c3");
    assert!(matches!(
        pick_member(Some(&view), "laptop"),
        Err(Response::Error {
            code: ErrorCode::InvalidRequest,
            ..
        })
    ));
    assert!(matches!(
        pick_member(Some(&view), "phone"),
        Err(Response::Error {
            code: ErrorCode::NotFound,
            ..
        })
    ));
    assert_eq!(pick_host(Some(&view), "build-01").unwrap(), "h1");
    assert!(matches!(
        pick_host(None, "build-01"),
        Err(Response::Error {
            code: ErrorCode::NotFound,
            ..
        })
    ));
}

mod fresh;
