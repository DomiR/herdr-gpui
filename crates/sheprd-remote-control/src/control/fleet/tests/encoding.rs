//! Canonical bytes: one encoding per operation, strict reading, bounds.

use super::*;

/// One operation of every body, signed in one fleet.
pub(super) fn every_body() -> Vec<Op> {
    let (a, b, p) = (computer(1), computer(2), phone(3));
    let mut log = found(&a);
    let mut ops = vec![
        log.ops()
            .find(|op| matches!(op.body(), Body::Found(_)))
            .unwrap()
            .clone(),
    ];
    ops.push(admit(&mut log, &a, &b));
    ops.push(admit(&mut log, &a, &p));
    let (intro, ca) = ca_host(40, HostKind::Sheprd);
    let host = intro.id().unwrap();
    ops.push(act(&mut log, &a, Body::HostAdd(intro)));
    ops.push(act(
        &mut log,
        &a,
        Body::Access(Access {
            host,
            member: b.id(),
            allow: false,
            rev: 1,
        }),
    ));
    let mut newer = record(host, 41, 2);
    newer.host = host;
    let signature = Some(raw_signature(&ca, &newer.message().unwrap()));
    ops.push(act(
        &mut log,
        &p,
        Body::HostSeen(HostSeen {
            record: newer,
            signature,
        }),
    ));
    ops.push(act(
        &mut log,
        &p,
        Body::Link(Link {
            host,
            device_key: compressed(&signing_key(70)).to_vec(),
        }),
    ));
    let mut rotated = b.bundle.clone();
    rotated.name = "studio".into();
    ops.push(act(&mut log, &b, Body::Rotate(rotated)));
    ops.push(act(&mut log, &a, Body::HostAdd(plain_host(42))));
    ops.push(act(
        &mut log,
        &b,
        Body::HostRemove(plain_host(42).id().unwrap()),
    ));
    let hash = log.state().hash().unwrap();
    ops.push(act(&mut log, &a, Body::Checkpoint(hash)));
    ops.push(act(
        &mut log,
        &p,
        Body::Remove(Remove {
            member: b.id(),
            also_members: Vec::new(),
            also_hosts: vec![host],
            reason: Reason::Lost,
        }),
    ));
    ops
}

#[test]
fn every_body_reads_back_as_written() {
    for op in every_body() {
        let bytes = op.encode();
        let read = Op::decode(&bytes).unwrap();
        assert_eq!(read, op);
        assert_eq!(read.id(), op.id());
        assert_eq!(read.encode(), bytes);
    }
}

#[test]
fn the_id_does_not_cover_the_signature() {
    let op = every_body().remove(1);
    let mut bytes = op.encode();
    let len = bytes.len();
    // Another valid encoding of a signature (or garbage) names the same
    // operation, so it can never look like a second one.
    bytes[len - 1] ^= 1;
    let changed = Op::decode(&bytes).unwrap();
    assert_eq!(changed.id(), op.id());
    let key = VerifyingKey::from_sec1_bytes(&computer(1).bundle.key).unwrap();
    assert_eq!(changed.verify(&key), Err(Error::Signature));
    assert_eq!(op.verify(&key), Ok(()));
}

#[test]
fn malformed_bytes_are_refused() {
    let op = every_body().remove(4);
    let bytes = op.encode();
    // Truncated, or with bytes left over.
    assert!(Op::decode(&bytes[..bytes.len() - 65]).is_err());
    let mut longer = bytes[..bytes.len() - 64].to_vec();
    longer.push(0);
    longer.extend_from_slice(op.signature());
    assert_eq!(Op::decode(&longer), Err(Error::Encoding));
    // Another domain or version.
    let mut other = bytes.clone();
    other[0] ^= 1;
    assert_eq!(Op::decode(&other), Err(Error::Encoding));
    let mut version = bytes.clone();
    version[DOMAIN.len()] = 2;
    assert_eq!(Op::decode(&version), Err(Error::Encoding));
    // An unknown body.
    let mut body = bytes;
    let at = DOMAIN.len() + 1 + 32 + 32 + 16;
    let parents = usize::from(body[at]);
    body[at + 1 + parents * 32] = 99;
    assert_eq!(Op::decode(&body), Err(Error::Encoding));
    assert_eq!(Op::decode(&[]), Err(Error::Bounds));
    assert_eq!(Op::decode(&vec![0; MAX_OP + 1]), Err(Error::Bounds));
}

#[test]
fn parents_are_a_sorted_set_within_bounds() {
    let a = computer(1);
    let log = found(&a);
    let one = log.heads()[0];
    let unsigned = |parents: Vec<OpId>| Unsigned {
        fleet: log.fleet(),
        signer: a.id(),
        seq: 2,
        issued_ms: NOW,
        parents,
        body: Body::HostRemove(HostId([1; 32])),
    };
    assert_eq!(
        unsigned(vec![one, one]).sign(&a.key).unwrap_err(),
        Error::Invalid
    );
    assert_eq!(
        unsigned(vec![OpId([9; 32]), OpId([1; 32])])
            .sign(&a.key)
            .unwrap_err(),
        Error::Invalid
    );
    assert_eq!(
        unsigned(Vec::new()).sign(&a.key).unwrap_err(),
        Error::Invalid
    );
    let many = (0..=MAX_PARENTS)
        .map(|index| OpId([u8::try_from(index).unwrap(); 32]))
        .collect();
    assert_eq!(unsigned(many).sign(&a.key).unwrap_err(), Error::Bounds);
}

#[test]
fn bodies_check_their_fields() {
    let mut phone_with_ssh = phone(3).bundle;
    phone_with_ssh.ssh = Some(ssh_line(3));
    assert_eq!(phone_with_ssh.check(), Err(Error::Invalid));
    let mut blank = computer(1).bundle;
    blank.name = "  ".into();
    assert_eq!(blank.check(), Err(Error::Invalid));
    let mut injected = computer(1).bundle;
    injected.ssh = Some(format!("{} extra\nssh-rsa AAAA", ssh_line(1)));
    assert_eq!(injected.check(), Err(Error::Invalid));
    // A host's record must name it, and be signed by its CA.
    let (mut intro, _) = ca_host(40, HostKind::Sheprd);
    intro.record.rev = 9;
    assert_eq!(intro.check(), Err(Error::SelfRecord));
    let (mut intro, _) = ca_host(40, HostKind::Sheprd);
    intro.record.host = HostId([0; 32]);
    assert_eq!(intro.check(), Err(Error::Invalid));
    // A CA key that is not in its certificate.
    let (mut intro, _) = ca_host(40, HostKind::Sheprd);
    if let Some(ca) = &mut intro.ca {
        ca.certificate = b"another certificate".to_vec();
    }
    assert_eq!(intro.check(), Err(Error::Invalid));
    // A plain host has no CA, and a Sheprd host must.
    let mut plain = plain_host(41);
    plain.kind = HostKind::Sheprd;
    assert_eq!(plain.check(), Err(Error::Invalid));
    let mut no_relays = plain_host(41);
    no_relays.record.relays.clear();
    assert_eq!(no_relays.check(), Err(Error::Invalid));
}

#[test]
fn a_state_reads_back_and_refuses_another_encoding() {
    let (a, b) = (computer(1), computer(2));
    let mut log = found(&a);
    admit(&mut log, &a, &b);
    act(&mut log, &a, Body::HostAdd(plain_host(40)));
    let bytes = log.state().encode().unwrap();
    assert_eq!(&State::decode(&bytes).unwrap(), log.state());
    let mut extra = bytes.clone();
    extra.push(0);
    assert_eq!(State::decode(&extra), Err(Error::Encoding));
    assert!(State::decode(&bytes[..bytes.len() - 1]).is_err());
}
