//! `saved::salvage`: what a device keeps of a saved copy that no longer
//! checks out, so the operations it signed survive starting again.

use super::*;

/// A fleet in which phone `p` signed a removal, and its saved bytes.
fn saved_with_phone_op() -> (Log, Device, Op, Vec<u8>) {
    let a = computer(1);
    let b = computer(2);
    let p = phone(3);
    let mut log = found(&a);
    admit(&mut log, &a, &b);
    admit(&mut log, &a, &p);
    let own = act(&mut log, &p, remove_body(&b));
    let bytes = saved::encode(&log).unwrap();
    (log, p, own, bytes)
}

#[test]
fn a_copy_whose_founding_signature_is_damaged_keeps_its_operations() {
    let (log, p, own, mut bytes) = saved_with_phone_op();
    let found = log.root().cloned().unwrap_or_else(|| {
        log.ops()
            .find(|op| matches!(op.body(), Body::Found(_)))
            .unwrap()
            .clone()
    });
    let encoded = found.encode();
    let at = bytes
        .windows(encoded.len())
        .position(|window| window == encoded.as_slice())
        .unwrap();
    bytes[at + encoded.len() - 1] ^= 1;
    assert!(saved::decode(&bytes, NOW).is_err());
    let salvaged = saved::salvage(&bytes);
    assert_eq!(salvaged.len(), log.ops().count());
    // Started again from a genuine founding operation, the phone's own
    // removal is checked and applies; the damaged copy is refused.
    let mut again = Log::found(found).unwrap();
    let results = again.insert_all(salvaged, NOW);
    assert!(
        results
            .iter()
            .any(|(id, result)| *id == own.id() && result.is_ok())
    );
    assert!(again.get(&own.id()).is_some());
    assert!(again.is_member(&p.id()));
}

#[test]
fn a_truncated_or_foreign_file_salvages_what_it_can() {
    let (log, _, _, bytes) = saved_with_phone_op();
    let all = saved::salvage(&bytes);
    assert_eq!(all.len(), log.ops().count());
    let some = saved::salvage(&bytes[..bytes.len() - 10]);
    assert_eq!(some.len(), all.len() - 1);
    assert!(saved::salvage(b"not a log").is_empty());
    assert!(saved::salvage(&[]).is_empty());
}

#[test]
fn a_clock_set_back_drops_nothing_a_saved_copy_holds() {
    let (log, _, own, bytes) = saved_with_phone_op();
    // Read an hour before the operations were signed.
    let reread = saved::decode(&bytes, NOW - 60 * 60 * 1000).unwrap();
    assert_eq!(reread.ops().count(), log.ops().count());
    assert!(reread.get(&own.id()).is_some());
}
