use super::*;

#[test]
fn a_founder_is_the_only_member_and_its_own_host() {
    let a = computer(1);
    let log = found(&a);
    assert_eq!(log.fleet(), FleetId(log.heads()[0].0));
    assert_eq!(log.state().members.keys().collect::<Vec<_>>(), [&a.id()]);
    assert_eq!(log.state().members[&a.id()].admitted_by, None);
    let own = a.bundle.host.as_ref().unwrap().id().unwrap();
    assert_eq!(log.state().hosts[&own].added_by, a.id());
    // A computer never reaches itself through the fleet.
    assert!(!log.state().allowed(&own, &a.id()));
}

#[test]
fn a_founder_must_be_a_computer_signing_its_own_bundle() {
    let p = phone(1);
    let op = Unsigned {
        fleet: FleetId([0; 32]),
        signer: p.id(),
        seq: 1,
        issued_ms: NOW,
        parents: Vec::new(),
        body: Body::Found(p.bundle.clone()),
    }
    .sign(&p.key)
    .unwrap();
    assert_eq!(Log::found(op).unwrap_err(), Error::Invalid);

    let (a, b) = (computer(1), computer(2));
    let op = Unsigned {
        fleet: FleetId([0; 32]),
        signer: a.id(),
        seq: 1,
        issued_ms: NOW,
        parents: Vec::new(),
        body: Body::Found(a.bundle.clone()),
    }
    .sign(&b.key)
    .unwrap();
    assert_eq!(Log::found(op).unwrap_err(), Error::Signature);
}

#[test]
fn members_admit_computers_and_phones_and_both_directions_open() {
    let (a, b, p) = (computer(1), computer(2), phone(3));
    let mut log = found(&a);
    admit(&mut log, &a, &b);
    // Phones admit too, after an approval on the phone.
    admit(&mut log, &b, &p);
    let state = log.state();
    assert_eq!(state.members.len(), 3);
    assert_eq!(state.members[&p.id()].admitted_by, Some(b.id()));
    let host_a = a.bundle.host.as_ref().unwrap().id().unwrap();
    let host_b = b.bundle.host.as_ref().unwrap().id().unwrap();
    assert!(state.allowed(&host_a, &b.id()));
    assert!(state.allowed(&host_b, &a.id()));
    // A phone never gets SSH.
    assert!(!state.allowed(&host_a, &p.id()));
    assert_eq!(state.grants(&host_a), [b.id()]);
}

#[test]
fn an_admission_needs_the_joiners_own_signature() {
    let (a, b, c) = (computer(1), computer(2), computer(3));
    let mut log = found(&a);
    let message = b.bundle.join_message(&log.fleet()).unwrap();
    // Signed by someone else: a member cannot admit a key its owner did
    // not hold.
    let body = Body::Admit(Admit {
        bundle: b.bundle.clone(),
        method: Method::Ssh,
        joiner_signature: raw_signature(&c.key, &message),
    });
    let op = sign(&log, &a, body);
    assert_eq!(log.insert(op, NOW), Err(Error::JoinSignature));
    // For another fleet.
    let other = found(&c);
    let message = b.bundle.join_message(&other.fleet()).unwrap();
    let body = Body::Admit(Admit {
        bundle: b.bundle.clone(),
        method: Method::Ssh,
        joiner_signature: raw_signature(&b.key, &message),
    });
    let op = sign(&log, &a, body);
    assert_eq!(log.insert(op, NOW), Err(Error::JoinSignature));
    assert_eq!(log.state().members.len(), 1);
}

#[test]
fn a_non_member_signs_nothing_that_counts() {
    let (a, outsider, b) = (computer(1), computer(9), computer(2));
    let mut log = found(&a);
    let op = sign(&log, &outsider, admit_body(&log, &b));
    assert_eq!(log.insert(op, NOW), Err(Error::Signer));
    let op = sign(&log, &outsider, Body::HostAdd(plain_host(40)));
    assert_eq!(log.insert(op, NOW), Err(Error::Signer));
    assert!(log.state().hosts.len() == 1);
}

#[test]
fn a_forged_signature_is_refused() {
    let (a, b) = (computer(1), computer(2));
    let mut log = found(&a);
    // Signed by b, claiming to be a.
    let op = log
        .draft(a.id(), NOW, Body::HostAdd(plain_host(40)))
        .sign(&b.key)
        .unwrap();
    assert_eq!(log.insert(op, NOW), Err(Error::Signature));
}

#[test]
fn what_each_kind_of_member_may_sign() {
    let (a, p) = (computer(1), phone(2));
    let mut log = found(&a);
    admit(&mut log, &a, &p);
    // A phone does not add hosts or set access.
    let op = sign(&log, &p, Body::HostAdd(plain_host(40)));
    assert_eq!(log.insert(op, NOW), Err(Error::Permission));
    let host = a.bundle.host.as_ref().unwrap().id().unwrap();
    let op = sign(
        &log,
        &p,
        Body::Access(Access {
            host,
            member: a.id(),
            allow: false,
            rev: 1,
        }),
    );
    assert_eq!(log.insert(op, NOW), Err(Error::Permission));
    // A computer does not sign a phone's links.
    let op = sign(
        &log,
        &a,
        Body::Link(Link {
            host,
            device_key: compressed(&signing_key(77)).to_vec(),
        }),
    );
    assert_eq!(log.insert(op, NOW), Err(Error::Permission));
    // A phone removes hosts, and signs its own links.
    act(
        &mut log,
        &p,
        Body::Link(Link {
            host,
            device_key: compressed(&signing_key(77)).to_vec(),
        }),
    );
    assert_eq!(log.state().links.len(), 1);
}

#[test]
fn a_rotation_keeps_the_fleet_key() {
    let (a, b) = (computer(1), computer(2));
    let mut log = found(&a);
    let mut rotated = a.bundle.clone();
    rotated.ssh = Some(ssh_line(42));
    act(&mut log, &a, Body::Rotate(rotated.clone()));
    assert_eq!(log.state().members[&a.id()].bundle, rotated);
    // A new fleet key is a new member, never a rotation.
    let op = sign(&log, &a, Body::Rotate(b.bundle.clone()));
    assert_eq!(log.insert(op, NOW), Err(Error::Permission));
}

#[test]
fn sequence_numbers_climb_and_clocks_are_checked() {
    let a = computer(1);
    let mut log = found(&a);
    let mut draft = log.draft(a.id(), NOW, Body::HostAdd(plain_host(40)));
    draft.seq = 1;
    assert_eq!(
        log.insert(draft.sign(&a.key).unwrap(), NOW),
        Err(Error::Sequence)
    );
    let draft = log.draft(a.id(), NOW + MAX_SKEW_MS + 1, Body::HostAdd(plain_host(40)));
    assert_eq!(
        log.insert(draft.sign(&a.key).unwrap(), NOW),
        Err(Error::Future)
    );
    // An old date is fine: a member that slept a week still brings valid
    // operations.
    let draft = log.draft(
        a.id(),
        NOW - 7 * 24 * 3_600_000,
        Body::HostAdd(plain_host(40)),
    );
    assert_eq!(
        log.insert(draft.sign(&a.key).unwrap(), NOW),
        Ok(Inserted::Applied)
    );
}

#[test]
fn another_fleets_operation_is_refused() {
    let (a, b) = (computer(1), computer(2));
    let mut log = found(&a);
    let other = found(&b);
    let op = sign(&other, &b, Body::HostAdd(plain_host(40)));
    assert_eq!(log.insert(op, NOW), Err(Error::Fleet));
}

#[test]
fn the_fleet_has_room_for_sixteen_computers() {
    let a = computer(1);
    let mut log = found(&a);
    for seed in 2..=u8::try_from(MAX_COMPUTERS).unwrap() {
        admit(&mut log, &a, &computer(seed));
    }
    let op = sign(&log, &a, admit_body(&log, &computer(200)));
    assert_eq!(log.insert(op, NOW), Err(Error::Full));
    // Phones are counted on their own.
    admit(&mut log, &a, &phone(201));
}

#[test]
fn membership_operations_are_bounded_between_checkpoints() {
    let a = computer(1);
    let mut log = found(&a);
    let mut bundle = a.bundle.clone();
    for _ in 1..MAX_MEMBERSHIP_OPS {
        bundle.tunnel[0] = bundle.tunnel[0].wrapping_add(1);
        act(&mut log, &a, Body::Rotate(bundle.clone()));
    }
    bundle.tunnel[0] = bundle.tunnel[0].wrapping_add(1);
    let op = sign(&log, &a, Body::Rotate(bundle));
    assert_eq!(log.insert(op, NOW), Err(Error::Full));
    // The log is as it was, and takes other work.
    assert_eq!(log.ops().count(), MAX_MEMBERSHIP_OPS);
    act(&mut log, &a, Body::HostAdd(plain_host(40)));
}
