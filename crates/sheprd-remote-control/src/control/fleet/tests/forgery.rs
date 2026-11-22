//! Forgeries in a member's name are refused before they are charged to
//! it, so they cannot use up its share of the log.

use super::*;

#[test]
fn forged_removals_do_not_crowd_out_a_genuine_one() {
    let (a, b, c) = (computer(1), computer(2), computer(3));
    let mut log = found(&a);
    admit(&mut log, &a, &b);
    admit(&mut log, &a, &c);
    let genuine = sign(&log, &b, remove_body(&c));
    // Forgeries claiming b, signed with c's key, all ordered before the
    // genuine removal, enough to fill b's share.
    let mut forged = Vec::new();
    let mut seq = 100;
    while forged.len() <= MAX_REMOVALS_PER_SIGNER {
        seq += 1;
        let op = Unsigned {
            seq,
            ..genuine.unsigned().clone()
        }
        .sign(&c.key)
        .unwrap();
        if op.id() < genuine.id() {
            forged.push(op);
        }
    }
    let mut batch = forged.clone();
    batch.push(genuine.clone());
    for (id, result) in log.insert_all(batch, NOW) {
        if id == genuine.id() {
            assert_eq!(result, Ok(Inserted::Applied));
        } else {
            assert_eq!(result, Err(Error::Signature));
        }
    }
    assert!(!log.state().members.contains_key(&c.id()));
}
