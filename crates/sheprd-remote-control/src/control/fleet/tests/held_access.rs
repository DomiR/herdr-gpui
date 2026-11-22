//! A held plain host's access file pauses SSH and says nothing else: each
//! member's allow is its register's, so an agent that held it gives the
//! same access back when the hold lifts, with no new `Access`.

use super::*;
use crate::control::fleet::access::{self as host_access, Id};
use sheprd_relay_agent::access::Access as AgentAccess;

fn never(_: &OpId) -> bool {
    false
}

#[test]
fn a_hold_that_lifts_gives_members_their_access_back() {
    let (a, b, c) = (computer(1), computer(2), computer(3));
    let mut log = found(&a);
    admit(&mut log, &a, &b);
    admit(&mut log, &a, &c);
    let intro = plain_host(40);
    let host = intro.id().unwrap();
    let unseen = log.heads().to_vec();
    act(&mut log, &a, Body::HostAdd(intro.clone()));
    // c is denied this host; a and b are not.
    act(
        &mut log,
        &a,
        Body::Access(Access {
            host,
            member: c.id(),
            allow: false,
            rev: 1,
        }),
    );
    let before = host_access::compute(&log, &host, [], never).unwrap();
    assert!(!before.paused);

    // b adds the host again with other keys, not having seen a's: held.
    let mut held_log = copy(&log);
    let mut other = intro;
    other.record.ssh_host_keys.push(ssh_line(251));
    let seq = held_log.state().members[&b.id()].seq + 1;
    let add = sign_on(&held_log, &b, &unseen, seq, Body::HostAdd(other));
    apply(&mut held_log, add);
    assert!(held_log.state().hosts[&host].held);
    let paused = host_access::compute(&held_log, &host, [], never).unwrap();
    assert!(paused.paused);
    // The registers are as they were: a hold is no deny.
    for (member, allow) in [(&a, true), (&b, true), (&c, false)] {
        assert_eq!(paused.members[&Id(member.id().0)].allow, allow);
    }

    // The agent took the paused file: no one reaches SSH.
    let agent = AgentAccess::merge(Some(&before), paused).unwrap();
    assert_eq!(agent.allowed().count(), 0);
    // Who may suspend whom stands: c is still the only one not granted.
    let granted: Vec<Id> = agent.granted().map(|(id, _)| *id).collect();
    assert_eq!(granted.len(), 2);
    assert!(!granted.contains(&Id(c.id().0)));

    // The hold lifts with no new `Access`: a and b are back.
    let lifted = AgentAccess::merge(Some(&agent), before).unwrap();
    let allowed: Vec<Id> = lifted.allowed().map(|(id, _)| *id).collect();
    assert_eq!(allowed, granted);
}
