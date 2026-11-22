//! `fleet remove` and `fleet remove-host` resolve against the daemon's
//! answer, which the server reads for each call, not Settings' cache.
use super::*;

#[test]
fn a_removal_is_resolved_against_the_daemons_answer() {
    let mut fresh = view();
    fresh.members.push(MemberView {
        id: "d4".into(),
        name: "new-mini".into(),
        kind: Kind::Computer,
        admitted_by: Some("a1".into()),
    });
    assert_eq!(
        pending(Some(&fresh), None, Target::Member("new-mini".into())).unwrap(),
        FleetPending::Remove("d4".into())
    );
    assert_eq!(
        pending(Some(&fresh), None, Target::Host("build-01".into())).unwrap(),
        FleetPending::RemoveHost("h1".into())
    );
}

#[test]
fn a_daemon_that_cannot_be_read_fails_the_removal() {
    assert!(matches!(
        pending(
            None,
            Some("refused".into()),
            Target::Member("studio".into())
        ),
        Err(Response::Error {
            code: ErrorCode::Failed,
            ..
        })
    ));
    assert!(matches!(
        pending(None, None, Target::Member("studio".into())),
        Err(Response::Error {
            code: ErrorCode::NotFound,
            ..
        })
    ));
}
