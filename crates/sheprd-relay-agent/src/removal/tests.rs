#![allow(clippy::unwrap_used)]

use super::*;

/// The bytes of a removal by `signer` of `targets` in `fleet`, laid out as
/// the fleet log lays one out, with a signature no one checks here.
pub(crate) fn remove_op(fleet: [u8; 32], signer: [u8; 32], targets: &[[u8; 32]]) -> Vec<u8> {
    let mut bytes = DOMAIN.to_vec();
    bytes.push(VERSION);
    bytes.extend_from_slice(&fleet);
    bytes.extend_from_slice(&signer);
    bytes.extend_from_slice(&7u64.to_be_bytes());
    bytes.extend_from_slice(&1_000u64.to_be_bytes());
    bytes.push(1);
    bytes.extend_from_slice(&[0xee; 32]);
    bytes.push(REMOVE);
    bytes.extend_from_slice(&targets[0]);
    bytes.extend_from_slice(&u16::try_from(targets.len() - 1).unwrap().to_be_bytes());
    targets[1..]
        .iter()
        .for_each(|target| bytes.extend_from_slice(target));
    bytes.extend_from_slice(&1u16.to_be_bytes());
    bytes.extend_from_slice(&[0xdd; 32]);
    bytes.push(3);
    bytes.extend_from_slice(&[0x55; SIGNATURE_LEN]);
    bytes
}

type Raw = Message<[u8; 32]>;

#[test]
fn a_removal_is_read_with_its_targets_and_id() {
    let op = remove_op([1; 32], [2; 32], &[[3; 32], [4; 32]]);
    let removed = peek(&op).unwrap().unwrap();
    assert_eq!(removed.fleet, [1; 32]);
    assert_eq!(removed.signer, [2; 32]);
    assert_eq!(removed.seq, 7);
    assert_eq!(removed.targets, [[3; 32], [4; 32]]);
    let signed = &op[..op.len() - SIGNATURE_LEN];
    assert_eq!(removed.id, <[u8; 32]>::from(Sha256::digest(signed)));
}

#[test]
fn another_operation_is_no_removal() {
    let mut op = remove_op([1; 32], [2; 32], &[[3; 32]]);
    let tag = DOMAIN.len() + 1 + 32 + 32 + 16 + 1 + 32;
    op[tag] = 6;
    assert_eq!(peek(&op), Ok(None));
}

#[test]
fn bytes_left_over_or_cut_short_are_refused() {
    let op = remove_op([1; 32], [2; 32], &[[3; 32]]);
    let mut longer = op.clone();
    longer.insert(op.len() - SIGNATURE_LEN, 0);
    assert_eq!(peek(&longer), Err(Error::Encoding));
    assert_eq!(peek(&op[..op.len() - 1]), Err(Error::Encoding));
    assert_eq!(peek(&op[..10]), Err(Error::Bounds));
    let mut other = op;
    other[0] = b'x';
    assert_eq!(peek(&other), Err(Error::Encoding));
}

#[test]
fn an_operation_over_its_bound_is_refused() {
    assert_eq!(peek(&vec![0; MAX_OP + 1]), Err(Error::Bounds));
}

#[test]
fn messages_read_back_as_written() {
    for message in [
        Raw::Removal(vec![vec![1, 2, 3], Vec::new()]),
        Raw::Want(vec![[9; 32]]),
        Raw::Held(vec![[7; 32], [8; 32]]),
        Raw::Held(Vec::new()),
    ] {
        let bytes = message.encode().unwrap();
        assert_eq!(Raw::decode(&bytes).unwrap(), message);
        let frame = message.frame().unwrap();
        let length = frame_length(frame[..4].try_into().unwrap()).unwrap();
        assert_eq!(&frame[4..], &bytes[..]);
        assert_eq!(length, bytes.len());
    }
}

#[test]
fn messages_over_their_bounds_are_refused() {
    let too_many = Raw::Held(vec![[1; 32]; MAX_OPS + 1]);
    assert_eq!(too_many.encode(), Err(Error::Bounds));
    let too_big = Raw::Removal(vec![vec![0; MAX_OP + 1]]);
    assert_eq!(too_big.encode(), Err(Error::Bounds));
    assert_eq!(
        frame_length(u32::try_from(MAX_FRAME + 1).unwrap().to_be_bytes()),
        Err(Error::Bounds)
    );
    let mut count = vec![HELD];
    count.extend_from_slice(&u16::try_from(MAX_OPS + 1).unwrap().to_be_bytes());
    assert_eq!(Raw::decode(&count), Err(Error::Bounds));
}

#[test]
fn malformed_messages_are_refused() {
    assert_eq!(Raw::decode(&[]), Err(Error::Encoding));
    assert_eq!(Raw::decode(&[9, 0, 0]), Err(Error::Encoding));
    assert_eq!(Raw::decode(&[HELD, 0, 1, 1]), Err(Error::Encoding));
    assert_eq!(Raw::decode(&[HELD, 0, 0, 1]), Err(Error::Encoding));
}

#[test]
fn a_proof_checks_out_only_for_its_key_and_handshake() {
    let key = SigningKey::from_slice(&[7; 32]).unwrap();
    let other = SigningKey::from_slice(&[8; 32]).unwrap();
    let public = key.verifying_key().to_sec1_point(true);
    let proof = prove(&key, &[1; 32]);
    assert!(verify(public.as_bytes(), &[1; 32], &proof));
    assert!(!verify(public.as_bytes(), &[2; 32], &proof));
    assert!(!verify(
        public.as_bytes(),
        &[1; 32],
        &prove(&other, &[1; 32])
    ));
    assert!(!verify(&[0; 33], &[1; 32], &proof));
    assert!(!verify(public.as_bytes(), &[1; 32], &[0; PROOF_LEN]));
}
