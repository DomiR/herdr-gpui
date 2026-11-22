//! Joining the fleet over Bluetooth in one step, the daemon's side
//! (`docs/relay-plan.md`, Adding a nearby Sheprd from another network;
//! `control::fleet::bluetooth` for what travels).
//!
//! With Pair device open, this computer names its fleet in the sealed
//! offer ([`hint`]). A desktop's claim may then ask to join that fleet,
//! or invite this computer into its own ([`check_claim`], before the
//! token is spent). Nothing happens until the person here allows the
//! pairing with the fleet line ticked, for the fleet the dialog showed
//! ([`allowed`]): then this computer admits the joiner, or signs its own
//! bundle, bound to the pairing, for the claimant to admit.

mod error;

pub use error::Error;

use super::{Fleet, me};
use crate::{
    auth::devices::RelayKeys,
    control::fleet::{
        Admit, Body, Bundle, Kind, Method,
        bluetooth::{ApprovedFleet, ClaimFleet, Hint, joining, pairing_digest},
        sync,
    },
    control::{PairingId, PairingState},
    ssh_access::SshKey,
    web::State,
};

/// What a desktop's claim asked of the fleet, checked.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum Claimed {
    /// The claimant joins this computer's fleet with `bundle`, whose
    /// keys are the ones it claimed with.
    Join {
        fleet: crate::control::fleet::FleetId,
        bundle: Box<Bundle>,
        signature: [u8; 64],
    },
    /// The claimant invites this computer into its fleet.
    Invite {
        hint: Hint,
        /// The pairing's token, hashed: what this computer's bundle is
        /// bound to if the person agrees.
        pairing: [u8; 32],
    },
}

impl Claimed {
    /// What the pairing dialog shows of it.
    pub(crate) fn asked(&self) -> crate::control::fleet::bluetooth::Asked {
        use crate::control::fleet::bluetooth::Asked;
        match self {
            Self::Join { fleet, .. } => Asked::Join {
                fleet: fleet.to_string(),
            },
            Self::Invite { hint, .. } => Asked::Invite(hint.clone()),
        }
    }
}

/// This computer's fleet as a nearby offer names it: only while it is a
/// live computer member, never as a shared host or once removed.
pub(crate) fn hint(fleet: &Fleet) -> Option<Hint> {
    if fleet.host().is_some() {
        return None;
    }
    let inner = fleet.inner();
    let log = inner.log.as_ref()?;
    if !log.is_member(&fleet.me()) {
        return None;
    }
    let state = log.state();
    let computers = state
        .members
        .values()
        .filter(|member| member.bundle.kind == Kind::Computer)
        .count();
    let own = state
        .members
        .values()
        .filter(|member| member.bundle.host.is_some())
        .count();
    Some(Hint {
        fleet: log.fleet().to_string(),
        computers: u16::try_from(computers).unwrap_or(u16::MAX),
        hosts: u16::try_from(state.hosts.len().saturating_sub(own)).unwrap_or(u16::MAX),
    })
}

/// Checks what a desktop's claim asks of the fleet, before its token is
/// spent. A join must be for this computer's own fleet, bound to this
/// pairing's `token`, by a key never removed, and name the very relay and
/// SSH keys the claim carries, so admitting it lets in no key but the one
/// that compared the code. An invitation only needs to name a fleet: it
/// grants nothing until the person here agrees.
pub(crate) fn check_claim(
    fleet: &Fleet,
    asked: ClaimFleet,
    token: &[u8; 32],
    relay: &RelayKeys,
    ssh: &SshKey,
) -> super::Result<Claimed> {
    let pairing = pairing_digest(token);
    match asked {
        ClaimFleet::Invite(hint) => {
            if !hint.valid() {
                return Err(crate::control::fleet::Error::Encoding.into());
            }
            Ok(Claimed::Invite { hint, pairing })
        }
        ClaimFleet::Join(joining) => {
            let id = hint(fleet).ok_or(Error::NotMember)?.id()?;
            let bundle = joining.check(&id, &pairing)?;
            let claimed = Some(bundle.dial) == relay.dial_key()
                && Some(bundle.tunnel) == relay.tunnel_key()
                && bundle.ssh.as_deref() == Some(ssh.as_str());
            if !claimed {
                return Err(Error::NotClaimant.into());
            }
            if fleet
                .inner()
                .log
                .as_ref()
                .is_some_and(|log| log.state().tombstones.contains(&bundle.id()))
            {
                return Err(Error::Removed.into());
            }
            let signature = sync::bytes(&joining.signature)?
                .try_into()
                .map_err(|_| crate::control::fleet::Error::JoinSignature)?;
            Ok(Claimed::Join {
                fleet: id,
                bundle: Box::new(bundle),
                signature,
            })
        }
    }
}

/// Acts on the person's Allow with the fleet line ticked for `agreed`,
/// the fleet the dialog showed, once the pairing itself is approved: admits
/// the joiner, or signs this computer's bundle for the claimant's fleet.
/// What the claimant then learns. Blocking: it writes the log.
pub(crate) fn allowed(
    state: &State,
    fleet: &Fleet,
    claimed: Claimed,
    agreed: &str,
    now_ms: u64,
) -> super::Result<ApprovedFleet> {
    match claimed {
        Claimed::Join {
            fleet: id,
            bundle,
            signature,
        } => {
            // The dialog showed this fleet, and this computer is still a
            // live member of it.
            let current = hint(fleet).ok_or(Error::NotMember)?;
            if current.fleet != agreed || current.fleet != id.to_string() {
                return Err(Error::Changed.into());
            }
            let member = bundle.id();
            let admitted = fleet
                .inner()
                .log
                .as_ref()
                .is_some_and(|log| log.is_member(&member));
            if !admitted {
                fleet.sign(
                    Body::Admit(Admit {
                        bundle: *bundle,
                        method: Method::Bluetooth,
                        joiner_signature: signature,
                    }),
                    now_ms,
                )?;
            }
            Ok(ApprovedFleet::Joined {
                fleet: current.fleet,
            })
        }
        Claimed::Invite {
            hint: invited,
            pairing,
        } => {
            if invited.fleet != agreed {
                return Err(Error::Changed.into());
            }
            // One fleet per computer: one already in a fleet pairs only.
            if fleet.in_fleet()? {
                return Err(super::Error::InFleet);
            }
            let id = invited.id()?;
            let bundle = me::bundle(state, fleet, now_ms)?;
            Ok(ApprovedFleet::Joining(joining(
                fleet.key(),
                &id,
                &pairing,
                &bundle,
            )?))
        }
    }
}

/// Answers pairing `id` as the person did with `answer`, then, when they
/// allowed it with the fleet line ticked for `agreed` and its claim asked
/// something of the fleet, acts on that ([`allowed`]). The claim reads as
/// waiting until the fleet acted, so the claimant never takes a plain
/// approval for the whole answer. A fleet that cannot act leaves the
/// pairing as it was approved: two computers paired, nothing joined.
/// Blocking.
pub(crate) fn answer(
    state: &State,
    id: &PairingId,
    allow: bool,
    agreed: Option<String>,
    now_ms: u64,
    answer: impl FnOnce() -> crate::auth::Result<PairingState>,
) -> crate::auth::Result<PairingState> {
    let acting = match (allow, agreed, &state.fleet) {
        (true, Some(agreed), Some(fleet)) => state
            .auth()
            .pairings()
            .asked_fleet(id)
            .map(|(device, claimed)| (device, claimed, agreed, fleet)),
        _ => None,
    };
    let Some((device, claimed, agreed, fleet)) = acting else {
        return answer();
    };
    state.auth().pairings().settling_fleet(device.clone());
    let answered = answer();
    let outcome = match &answered {
        Ok(PairingState::Approved { device: approved }) if *approved == device => allowed(
            state, fleet, claimed, &agreed, now_ms,
        )
        .inspect_err(|error| {
            tracing::warn!(%error, "a pairing was allowed, but the fleet could not act on it");
        })
        .ok(),
        _ => None,
    };
    state.auth().pairings().settled_fleet(device, outcome);
    answered
}
