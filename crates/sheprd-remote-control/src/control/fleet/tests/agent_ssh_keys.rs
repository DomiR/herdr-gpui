//! A computer whose SSH key is not Ed25519 stays in its plain hosts'
//! access files, so a deny and a rotation still reach their agents.

use super::*;
use crate::control::fleet::access::{self as host_access, Id};
use sheprd_relay_agent::access::Access as AgentAccess;

#[test]
fn a_member_rotating_to_a_key_agents_cannot_write_is_denied_and_its_old_keys_retired() {
    let (a, b) = (computer(1), computer(2));
    let mut log = found(&a);
    admit(&mut log, &a, &b);
    let intro = plain_host(40);
    let host = intro.id().unwrap();
    act(&mut log, &a, Body::HostAdd(intro));
    let never = |_: &OpId| false;
    // The agent holds b's first keys, allowed.
    let held = host_access::compute(&log, &host, [], never).unwrap();
    let member = Id(b.id().0);
    assert!(held.allowed().any(|(id, _)| *id == member));
    // b rotates to an RSA key, then is denied this host.
    let mut bundle = b.bundle.clone();
    bundle.ssh = Some("ssh-rsa AAAAB3NzaC1yc2E".to_owned());
    bundle.tunnel = [0x63; 32];
    act(&mut log, &b, Body::Rotate(bundle));
    act(
        &mut log,
        &a,
        Body::Access(Access {
            host,
            member: b.id(),
            allow: false,
            rev: 1,
        }),
    );
    let file = host_access::compute(&log, &host, [], never).unwrap();
    let listed = &file.members[&member];
    assert_eq!(listed.ssh, None);
    assert!(!listed.allow);
    assert_eq!(listed.tunnel, [0x63; 32]);
    // The agent takes the deny and the new keys; the old ones are former.
    let merged = AgentAccess::merge(Some(&held), file).unwrap();
    let kept = &merged.members[&member];
    assert!(!kept.allow);
    assert_eq!(kept.ssh, None);
    assert_eq!(kept.tunnel, [0x63; 32]);
    assert!(
        kept.former
            .iter()
            .any(|pair| pair.tunnel == b.bundle.tunnel)
    );
    assert!(!merged.allowed().any(|(id, _)| *id == member));
}

#[test]
fn an_allowed_member_without_an_ed25519_key_never_reaches_ssh() {
    let (a, b) = (computer(1), computer(2));
    let mut log = found(&a);
    admit(&mut log, &a, &b);
    let intro = plain_host(40);
    let host = intro.id().unwrap();
    act(&mut log, &a, Body::HostAdd(intro));
    let mut bundle = b.bundle.clone();
    bundle.ssh = Some("ssh-rsa AAAAB3NzaC1yc2E".to_owned());
    act(&mut log, &b, Body::Rotate(bundle));
    let file = host_access::compute(&log, &host, [], |_: &OpId| false).unwrap();
    let member = Id(b.id().0);
    assert!(file.members[&member].allow);
    assert!(!file.allowed().any(|(id, _)| *id == member));
    // The file reads back as the agent parses it.
    let read = AgentAccess::parse(&file.to_bytes().unwrap()).unwrap();
    assert_eq!(read, file);
}
