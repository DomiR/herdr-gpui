//! What an approved claim says about the fleet, checked before Sheprd acts
//! on it (`control::fleet::bluetooth`). Anything that does not check out
//! is dropped: the two computers are paired, and nothing joins.

use super::Opened;
use crate::control::fleet::bluetooth::{ApprovedFleet, ClaimFleet, pairing_digest};

/// What became of the fleet, when the answer matches what the claim
/// asked: the fleet this computer asked to join, or the other computer's
/// bundle for the fleet it was invited to, bound to this pairing's token
/// and naming the very certificate authority, route, and tunnel identity
/// the code covered. A bundle for any other computer, or from another
/// pairing, is never handed on to be admitted.
pub(crate) fn checked(
    opened: &Opened,
    asked: Option<&ClaimFleet>,
    answered: Option<ApprovedFleet>,
) -> Option<ApprovedFleet> {
    match (asked?, answered?) {
        (ClaimFleet::Join(_), ApprovedFleet::Joined { fleet }) => {
            let named = opened.fleet.as_ref()?;
            (named.fleet.eq_ignore_ascii_case(&fleet)).then_some(ApprovedFleet::Joined { fleet })
        }
        (ClaimFleet::Invite(hint), ApprovedFleet::Joining(joining)) => {
            let bundle = joining
                .check(&hint.id().ok()?, &pairing_digest(&opened.token))
                .ok()?;
            let host = bundle.host.as_ref()?;
            let relay = opened.relay.as_ref()?;
            let same = host.ca.as_ref()?.fingerprint() == opened.fingerprint
                && host.record.route == relay.route
                && host.record.identity == relay.identity;
            same.then_some(ApprovedFleet::Joining(joining))
        }
        _ => None,
    }
}
