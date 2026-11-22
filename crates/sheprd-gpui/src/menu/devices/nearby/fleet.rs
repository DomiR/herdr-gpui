//! What pairing with a nearby Sheprd does for the fleet
//! (`docs/relay-plan.md`, Adding a nearby Sheprd from another network).
//! Once both screens show the code, the page knows both computers' fleets:
//! this one's from its daemon, the other's from the sealed offer. Where
//! the pair could end up in one fleet, the person here chooses first;
//! the `desktop-pair` child waits for that choice before it claims.

use super::{NearbySetup, Step};
use crate::SheprdWindow;
use crate::fleet::{Fleet, bluetooth};
use gpui::{App, Context};
use sheprd_remote_control::control::fleet::bluetooth::{ClaimFleet, Hint};

/// What the pair may do for the fleet, as the page offers it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Offer {
    /// Nothing to ask: no daemon view yet, or this computer holds a fleet
    /// it is no member of (shared as a host, or removed).
    None,
    /// Both are in the same fleet already.
    Same,
    /// This computer's fleet, which the other computer, in no fleet, may
    /// join: the person here chooses.
    Invite(Hint),
    /// The other computer's fleet, which this one, in no fleet, may join:
    /// the person here chooses.
    Join(Hint),
    /// Neither is in a fleet: the person here may found one.
    Found,
    /// Each is in its own fleet: the invitation goes, and the other
    /// computer pairs only, naming its own.
    Other(Hint),
}

impl Offer {
    /// What the pair may do, from this computer's `view` and whether it
    /// is a member there, and the fleet the other computer named.
    pub(super) fn of(
        view: Option<&sheprd_remote_control::control::fleet::View>,
        member: bool,
        theirs: Option<&Hint>,
    ) -> Self {
        match (view, member, theirs) {
            (Some(view), true, Some(theirs)) if theirs.fleet.eq_ignore_ascii_case(&view.fleet) => {
                Self::Same
            }
            (Some(view), true, Some(_)) => Self::Other(bluetooth::hint(view)),
            (Some(view), true, None) => Self::Invite(bluetooth::hint(view)),
            (Some(_), false, _) => Self::None,
            (None, _, Some(theirs)) => Self::Join(theirs.clone()),
            (None, _, None) => Self::Found,
        }
    }

    /// Whether the person here chooses before the claim goes.
    pub(super) fn asks(&self) -> bool {
        matches!(self, Self::Invite(_) | Self::Join(_) | Self::Found)
    }
}

/// What this computer offers with `theirs`, as the daemon last said.
pub(super) fn offer(cx: &App, theirs: Option<&Hint>) -> Offer {
    if !Fleet::read(cx) {
        return Offer::None;
    }
    Offer::of(Fleet::view(cx), Fleet::member_of(cx).is_some(), theirs)
}

impl SheprdWindow {
    /// Sends the claim at once when there is nothing to choose.
    pub(super) fn nearby_claim_unasked(&mut self) {
        let Some(setup) = &mut self.menu.nearby_setup else {
            return;
        };
        let Step::Code { offer, .. } = &setup.step else {
            return;
        };
        let fleet = match offer {
            Offer::Other(ours) => Some(ClaimFleet::Invite(ours.clone())),
            Offer::None | Offer::Same => None,
            Offer::Invite(_) | Offer::Join(_) | Offer::Found => return,
        };
        claim(setup, fleet);
    }

    /// The person's choice: `join` takes the page's offer, otherwise the
    /// two pair only.
    pub(super) fn choose_nearby_fleet(&mut self, join: bool, cx: &mut Context<Self>) {
        let Some(setup) = &mut self.menu.nearby_setup else {
            return;
        };
        let Step::Code {
            offer,
            pairing,
            decided,
            ..
        } = &mut setup.step
        else {
            return;
        };
        if *decided {
            return;
        }
        *decided = true;
        let attempt = setup.attempt;
        let (offer, pairing) = (offer.clone(), pairing.clone());
        match (join, offer) {
            (true, Offer::Invite(ours)) => claim(setup, Some(ClaimFleet::Invite(ours))),
            (true, Offer::Join(theirs)) => {
                let this = cx.entity().downgrade();
                bluetooth::joining(cx, theirs.fleet, pairing, move |joining, cx| {
                    let _ = this.update(cx, |view, cx| {
                        view.nearby_claim_with(attempt, joining.map(ClaimFleet::Join), cx);
                    });
                });
            }
            (true, Offer::Found) => {
                let this = cx.entity().downgrade();
                bluetooth::found(cx, move |founded, cx| {
                    let _ = this.update(cx, |view, cx| {
                        view.nearby_claim_with(attempt, founded.map(ClaimFleet::Invite), cx);
                    });
                });
            }
            _ => claim(setup, None),
        }
        cx.notify();
    }

    /// Sends the claim the daemon prepared for `attempt`, or stops with why
    /// it could not.
    fn nearby_claim_with(
        &mut self,
        attempt: u64,
        fleet: Result<ClaimFleet, crate::fleet::Error>,
        cx: &mut Context<Self>,
    ) {
        let Some(setup) = &mut self.menu.nearby_setup else {
            return;
        };
        // Cancelled, failed, or another computer picked meanwhile: the
        // child that waited for this is gone.
        if setup.attempt != attempt || !matches!(setup.step, Step::Code { .. }) {
            return;
        }
        match fleet {
            Ok(fleet) => claim(setup, Some(fleet)),
            Err(error) => {
                setup.error = Some(crate::i18n::fl!(
                    "devices-nearby-failed",
                    error = error.to_string()
                ));
                self.stop_nearby_pairing(cx);
            }
        }
        cx.notify();
    }
}

/// Hands the child its claim.
fn claim(setup: &mut NearbySetup, fleet: Option<ClaimFleet>) {
    if let Step::Code { decided, .. } = &mut setup.step {
        *decided = true;
    }
    if let Some(pairing) = &setup.pairing {
        pairing.claim(fleet);
    }
}

#[cfg(test)]
mod tests;
