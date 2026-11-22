//! The pairing the desktop dialog holds open: a single-use token in its QR,
//! a phone's claim of it, and the person's answer.
//!
//! Only the token's digest is kept. At most one pairing is open; minting
//! again rotates its token, voiding the previous one.

use super::{
    Error, PublicKey, Result, SECRET_LEN, constant_time_eq, decode_fixed, device_name, encode,
    random_bytes, random_id, sha256,
};
use crate::control::{DeviceId, Offer, PairingId, PairingState, fleet::bluetooth::ApprovedFleet};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

/// How long a minted token can be claimed.
pub(crate) const TOKEN_TTL_MS: u64 = 60_000;
/// How long a claimed pairing waits for the desktop's answer.
pub(crate) const APPROVAL_TTL_MS: u64 = 120_000;
/// Nearby tokens one pairing seals before the dialog must be opened again.
/// Each one is a fresh guess at the code for a man in the middle, and
/// keeps the pairing open a little longer, so both stay bounded.
pub(crate) const MAX_NEARBY_REVEALS: u8 = 3;
/// How long a pairing link's token can be claimed: the relays' pairing
/// dial lasts as long, and the link has to be sent to the other computer.
pub(crate) const LINK_TTL_MS: u64 = 120_000;
/// Finished claims whose outcome a phone can still poll.
const REMEMBERED_OUTCOMES: usize = 8;

/// A phone's claim, named by the daemon so the phone can poll its outcome.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub(crate) struct ClaimId(pub(crate) String);

/// What the phone learns when its claim is accepted.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub(crate) struct Claim {
    pub(crate) id: ClaimId,
    /// The matching code the phone shows beside the desktop's.
    pub(crate) code: String,
    /// When the desktop's answer stops being accepted.
    pub(crate) expires_ms: u64,
}

/// A device the desktop approved, for the device store to keep.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct NewDevice {
    /// The origin of the Home Screen app that claimed it, when that app was
    /// installed from another computer; admitted by CORS while the device
    /// stays paired.
    pub(crate) app: Option<String>,
    /// What a desktop's claim asked of the fleet, checked
    /// (`fleet::bluetooth`); acted on only by an Allow that agrees to it.
    pub(crate) fleet: Option<Box<crate::fleet::bluetooth::Claimed>>,
    pub(crate) id: DeviceId,
    pub(crate) name: String,
    pub(crate) public_key: PublicKey,
    /// The relay keys the phone sent with its claim, stored when approved.
    pub(crate) relay: Option<super::devices::RelayKeys>,
    /// A desktop's SSH key, let in through the relays when approved.
    pub(crate) ssh: Option<crate::ssh_access::SshKey>,
}

#[derive(Debug, Default)]
pub(crate) struct Pairings {
    /// What approved desktops learn about the fleet, oldest first.
    fleets: VecDeque<(DeviceId, ApprovedFleet)>,
    /// This computer's fleet, for the next nearby offer alone
    /// ([`Pairings::offer_hint`]).
    hint: Option<crate::control::fleet::bluetooth::Hint>,
    open: Option<Open>,
    /// Outcomes of finished claims, oldest first.
    outcomes: VecDeque<(ClaimId, PairingState)>,
    /// The devices whose approval the fleet is still acting on, once per
    /// answer acting: a claim reads as waiting until none is, so it never
    /// learns it was approved before it learns what became of the fleet,
    /// even when two answers race and one of them finds nothing to do.
    settling: Vec<DeviceId>,
}

#[derive(Debug)]
struct Open {
    id: PairingId,
    token_digest: [u8; 32],
    token_expires_ms: u64,
    /// The token a nearby phone was sealed, kept beside the QR's.
    nearby: Option<NearbyToken>,
    /// Nearby tokens sealed so far, kept when the QR rotates.
    nearby_reveals: u8,
    /// The token of the pairing link the dialog copied, kept when the QR
    /// rotates.
    link: Option<LinkToken>,
    phase: Phase,
}

/// A pairing link's token (`control::pairing_link`): claimed through the
/// relays, and its claim's code derived as a QR claim's is.
#[derive(Debug)]
struct LinkToken {
    digest: [u8; 32],
    expires_ms: u64,
}

/// Which of the open pairing's tokens a claim holds.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Held {
    Link,
    Nearby,
    Qr,
}

/// A token sealed to a nearby phone after it agreed on `code` with the
/// daemon: single use, and its claim carries that code rather than one
/// derived from the token.
#[derive(Debug)]
struct NearbyToken {
    digest: [u8; 32],
    code: String,
    expires_ms: u64,
}

impl NearbyToken {
    fn live(&self, now_ms: u64) -> bool {
        now_ms < self.expires_ms
    }
}

#[derive(Debug)]
enum Phase {
    Waiting,
    Claimed {
        claim: ClaimId,
        device: NewDevice,
        code: String,
        deadline_ms: u64,
    },
    /// Approved, denied, or expired; `claim` is `None` when the token
    /// expired unclaimed.
    Finished {
        claim: Option<ClaimId>,
        state: PairingState,
    },
}

impl Open {
    /// When the last token that may still be claimed expires.
    fn claimable_until(&self) -> u64 {
        let link = self.link.as_ref().map_or(0, |link| link.expires_ms);
        self.shown_until().max(link)
    }

    /// When the dialog mints its next QR: a link outlives the QR it was
    /// copied beside, and minting again keeps it.
    fn shown_until(&self) -> u64 {
        let nearby = self.nearby.as_ref().map_or(0, |nearby| nearby.expires_ms);
        self.token_expires_ms.max(nearby)
    }

    /// Which token `digest` is, if any.
    fn held(&self, digest: &[u8; 32]) -> Option<Held> {
        let nearby = self
            .nearby
            .as_ref()
            .is_some_and(|nearby| constant_time_eq(&nearby.digest, digest));
        let link = self
            .link
            .as_ref()
            .is_some_and(|link| constant_time_eq(&link.digest, digest));
        if nearby {
            Some(Held::Nearby)
        } else if link {
            Some(Held::Link)
        } else {
            constant_time_eq(&self.token_digest, digest).then_some(Held::Qr)
        }
    }

    /// Whether the `held` token may still be claimed.
    fn live(&self, held: Held, now_ms: u64) -> bool {
        match held {
            Held::Nearby => self.nearby.as_ref().is_some_and(|token| token.live(now_ms)),
            Held::Link => self
                .link
                .as_ref()
                .is_some_and(|link| now_ms < link.expires_ms),
            Held::Qr => now_ms < self.token_expires_ms,
        }
    }

    /// Whether minting again keeps this pairing's id: nobody claimed it.
    fn unclaimed(&self) -> bool {
        matches!(
            self.phase,
            Phase::Waiting | Phase::Finished { claim: None, .. }
        )
    }

    fn view(&self, now_ms: u64) -> PairingState {
        match &self.phase {
            Phase::Waiting => PairingState::Waiting {
                expires_in_ms: self.shown_until().saturating_sub(now_ms),
            },
            Phase::Claimed { device, code, .. } => PairingState::Claimed {
                device_name: device.name.clone(),
                code: code.clone(),
                app: device.app.clone(),
                ssh: device.ssh.is_some(),
                fleet: device
                    .fleet
                    .as_deref()
                    .map(crate::fleet::bluetooth::Claimed::asked),
            },
            Phase::Finished { state, .. } => state.clone(),
        }
    }
}

impl Pairings {
    /// Opens a pairing, or rotates the open one's token while nobody has
    /// claimed it. A claimed or finished pairing is replaced by a new one.
    pub(crate) fn mint(
        &mut self,
        now_ms: u64,
        origins: &[String],
        fingerprint: &str,
    ) -> Result<Offer> {
        let origin = origins.first().ok_or(Error::NoAddress)?;
        let token = random_bytes::<SECRET_LEN>()?;
        // Rotating the QR keeps a token already sealed to a nearby phone,
        // which claims it moments later.
        // So is a link the dialog copied, which takes longer to arrive.
        let (id, nearby, nearby_reveals, link) = match self.open.as_mut() {
            Some(open) if open.unclaimed() => (
                open.id.clone(),
                open.nearby.take(),
                open.nearby_reveals,
                open.link.take(),
            ),
            _ => (PairingId(random_id()?), None, 0, None),
        };
        self.close();
        let token_expires_ms = now_ms.saturating_add(TOKEN_TTL_MS);
        self.open = Some(Open {
            id: id.clone(),
            token_digest: sha256(&[&token]),
            token_expires_ms,
            nearby,
            nearby_reveals,
            link,
            phase: Phase::Waiting,
        });
        Ok(Offer {
            address_details: Default::default(),
            id,
            url: format!(
                "{origin}/pair#t={}&fp={}",
                encode(&token),
                fingerprint.replace(':', "")
            ),
            fingerprint: fingerprint.to_owned(),
            expires_in_ms: TOKEN_TTL_MS,
            addresses: origins.to_vec(),
        })
    }

    /// A phone claims the token with its name and public key. The token is
    /// used up only when the name and key are valid; claiming it twice
    /// voids the pairing.
    pub(crate) fn claim(
        &mut self,
        token: &str,
        device_name_input: &str,
        public_key: &str,
        now_ms: u64,
    ) -> Result<Claim> {
        let token: [u8; SECRET_LEN] = decode_fixed(token).map_err(|_| Error::InvalidToken)?;
        let digest = sha256(&[&token]);
        let Some(open) = self.open.as_mut() else {
            return Err(Error::InvalidToken);
        };
        let held = open.held(&digest).ok_or(Error::InvalidToken)?;
        refresh(open, &mut self.outcomes, now_ms);
        match &open.phase {
            Phase::Waiting => {}
            Phase::Finished { claim: None, .. } => return Err(Error::ExpiredToken),
            Phase::Finished { claim: Some(_), .. } => return Err(Error::AlreadyClaimed),
            Phase::Claimed { .. } => {
                settle(open, &mut self.outcomes, PairingState::Denied);
                return Err(Error::AlreadyClaimed);
            }
        }
        // The pairing outlives one token while another may be claimed.
        if !open.live(held, now_ms) {
            return Err(Error::ExpiredToken);
        }
        let name = device_name(device_name_input)?;
        let public_key = PublicKey::parse(public_key)?;
        let nonce = random_bytes::<SECRET_LEN>()?;
        let claim = ClaimId(random_id()?);
        let device = NewDevice {
            app: None,
            fleet: None,
            id: DeviceId(random_id()?),
            name,
            public_key,
            relay: None,
            ssh: None,
        };
        // Single use: a nearby token's code is the one both screens agreed on.
        let code = match open.nearby.take() {
            Some(token) if held == Held::Nearby => token.code,
            other => {
                open.nearby = other;
                matching_code(&token, device.public_key.as_bytes(), &nonce)
            }
        };
        let deadline_ms = now_ms.saturating_add(APPROVAL_TTL_MS);
        open.phase = Phase::Claimed {
            claim: claim.clone(),
            device,
            code: code.clone(),
            deadline_ms,
        };
        Ok(Claim {
            id: claim,
            code,
            expires_ms: deadline_ms,
        })
    }

    /// Whether `token` would be taken by a claim now, without spending it:
    /// what a claim checks before anything costly it carries, so a peer
    /// without the token learns nothing and costs nothing.
    pub(crate) fn holds(&mut self, token: &[u8; SECRET_LEN], now_ms: u64) -> bool {
        let digest = sha256(&[token]);
        let Some(open) = self.open.as_mut() else {
            return false;
        };
        refresh(open, &mut self.outcomes, now_ms);
        matches!(open.phase, Phase::Waiting)
            && open
                .held(&digest)
                .is_some_and(|held| open.live(held, now_ms))
    }

    /// Whether a pairing dialog is open and nobody claimed it: what nearby
    /// pairing needs before it starts.
    pub(crate) fn waiting(&mut self, now_ms: u64) -> bool {
        let Some(open) = self.open.as_mut() else {
            return false;
        };
        refresh(open, &mut self.outcomes, now_ms);
        matches!(open.phase, Phase::Waiting)
    }

    /// Keeps `token` as the open pairing's nearby token, claimable for
    /// [`TOKEN_TTL_MS`] with `code`; a newer one replaces it, at most
    /// [`MAX_NEARBY_REVEALS`] times for one pairing.
    pub(crate) fn set_nearby(
        &mut self,
        token: &[u8; SECRET_LEN],
        code: String,
        now_ms: u64,
    ) -> Result<()> {
        if !self.waiting(now_ms) {
            return Err(Error::NoPairing);
        }
        let open = self.open.as_mut().ok_or(Error::NoPairing)?;
        if open.nearby_reveals >= MAX_NEARBY_REVEALS {
            return Err(Error::NearbyExhausted);
        }
        open.nearby_reveals += 1;
        open.nearby = Some(NearbyToken {
            digest: sha256(&[token]),
            code,
            expires_ms: now_ms.saturating_add(TOKEN_TTL_MS),
        });
        Ok(())
    }

    /// Keeps `token` as pairing `id`'s link token, claimable for
    /// [`LINK_TTL_MS`]; a newer link replaces it. Returns how long it
    /// lasts.
    pub(crate) fn set_link(
        &mut self,
        id: &PairingId,
        token: &[u8; SECRET_LEN],
        now_ms: u64,
    ) -> Result<u64> {
        let open = find(&mut self.open, id)?;
        refresh(open, &mut self.outcomes, now_ms);
        if !matches!(open.phase, Phase::Waiting) {
            return Err(Error::NoPairing);
        }
        open.link = Some(LinkToken {
            digest: sha256(&[token]),
            expires_ms: now_ms.saturating_add(LINK_TTL_MS),
        });
        Ok(LINK_TTL_MS)
    }

    /// Keeps the relay keys the phone sent with `claim`, so approving it
    /// lets the phone through this computer's relays at once.
    pub(crate) fn claimed_relay(&mut self, claim: &ClaimId, keys: super::devices::RelayKeys) {
        if let Some(Open {
            phase: Phase::Claimed {
                claim: id, device, ..
            },
            ..
        }) = self.open.as_mut()
            && id == claim
        {
            device.relay = Some(keys);
        }
    }

    /// Keeps the SSH key a desktop sent with `claim`, so approving it lets
    /// that desktop in over SSH.
    pub(crate) fn claimed_ssh(&mut self, claim: &ClaimId, key: crate::ssh_access::SshKey) {
        if let Some(Open {
            phase: Phase::Claimed {
                claim: id, device, ..
            },
            ..
        }) = self.open.as_mut()
            && id == claim
        {
            device.ssh = Some(key);
        }
    }

    /// Keeps what a desktop's `claim` asked of the fleet, checked.
    pub(crate) fn claimed_fleet(
        &mut self,
        claim: &ClaimId,
        fleet: crate::fleet::bluetooth::Claimed,
    ) {
        if let Some(Open {
            phase: Phase::Claimed {
                claim: id, device, ..
            },
            ..
        }) = self.open.as_mut()
            && id == claim
        {
            device.fleet = Some(Box::new(fleet));
        }
    }

    /// The device the claim on pairing `id` would become, and what it
    /// asked of the fleet, while it waits for the person's answer.
    pub(crate) fn asked_fleet(
        &self,
        id: &PairingId,
    ) -> Option<(DeviceId, crate::fleet::bluetooth::Claimed)> {
        match self.open.as_ref() {
            Some(Open {
                id: open,
                phase: Phase::Claimed { device, .. },
                ..
            }) if open == id => device
                .fleet
                .clone()
                .map(|fleet| (device.id.clone(), *fleet)),
            _ => None,
        }
    }

    /// Names this computer's fleet in the next nearby offer, and that one
    /// only: an offer made where the fleet was not read names none.
    pub(crate) fn offer_hint(&mut self, hint: Option<crate::control::fleet::bluetooth::Hint>) {
        self.hint = hint;
    }

    pub(crate) fn take_hint(&mut self) -> Option<crate::control::fleet::bluetooth::Hint> {
        self.hint.take()
    }

    /// Holds `device`'s approval back from its claim until
    /// [`Pairings::settled_fleet`].
    pub(crate) fn settling_fleet(&mut self, device: DeviceId) {
        self.settling.push(device);
    }

    /// Lets `device`'s claim learn it was approved, with what became of
    /// the fleet, kept for the last few devices approved.
    pub(crate) fn settled_fleet(&mut self, device: DeviceId, fleet: Option<ApprovedFleet>) {
        if let Some(index) = self.settling.iter().position(|held| *held == device) {
            self.settling.swap_remove(index);
        }
        let Some(fleet) = fleet else {
            return;
        };
        if self.fleets.len() == REMEMBERED_OUTCOMES {
            self.fleets.pop_front();
        }
        self.fleets.push_back((device, fleet));
    }

    /// Whether the fleet is still acting on `device`'s approval.
    pub(crate) fn fleet_settling(&self, device: &DeviceId) -> bool {
        self.settling.contains(device)
    }

    /// What the approved `device` learns about the fleet, if anything.
    pub(crate) fn fleet_for(&self, device: &DeviceId) -> Option<ApprovedFleet> {
        self.fleets
            .iter()
            .rev()
            .find(|(id, _)| id == device)
            .map(|(_, fleet)| fleet.clone())
    }

    /// Notes that `claim` came from an app installed from another computer,
    /// at `origin`, so approving it admits that origin.
    pub(crate) fn claimed_from(&mut self, claim: &ClaimId, origin: String) {
        if let Some(Open {
            phase: Phase::Claimed {
                claim: id, device, ..
            },
            ..
        }) = self.open.as_mut()
            && id == claim
        {
            device.app = Some(origin);
        }
    }

    /// Where the pairing stands, for the desktop dialog.
    pub(crate) fn state(&mut self, id: &PairingId, now_ms: u64) -> Result<PairingState> {
        let open = find(&mut self.open, id)?;
        refresh(open, &mut self.outcomes, now_ms);
        Ok(open.view(now_ms))
    }

    /// Where a claim stands, for the phone that made it.
    pub(crate) fn claim_status(&mut self, id: &ClaimId, now_ms: u64) -> Result<PairingState> {
        if let Some(open) = self.open.as_mut() {
            refresh(open, &mut self.outcomes, now_ms);
            if matches!(&open.phase, Phase::Claimed { claim, .. } if claim == id) {
                return Ok(open.view(now_ms));
            }
        }
        self.outcomes
            .iter()
            .rev()
            .find(|(claim, _)| claim == id)
            .map(|(_, state)| state.clone())
            .ok_or(Error::UnknownClaim)
    }

    /// The desktop's answer to a claimed pairing. On approval `keep` stores
    /// the device before anyone can learn it was approved; when it fails the
    /// pairing is denied instead. Returns the approved device.
    pub(crate) fn answer(
        &mut self,
        id: &PairingId,
        allow: bool,
        now_ms: u64,
        keep: impl FnOnce(&NewDevice) -> Result<()>,
    ) -> Result<Option<NewDevice>> {
        let open = find(&mut self.open, id)?;
        refresh(open, &mut self.outcomes, now_ms);
        let Phase::Claimed { device, .. } = &open.phase else {
            return Err(Error::NotClaimed);
        };
        if !allow {
            settle(open, &mut self.outcomes, PairingState::Denied);
            return Ok(None);
        }
        if let Err(error) = keep(device) {
            settle(open, &mut self.outcomes, PairingState::Denied);
            return Err(error);
        }
        let approved = PairingState::Approved {
            device: device.id.clone(),
        };
        match settle(open, &mut self.outcomes, approved) {
            Phase::Claimed { device, .. } => Ok(Some(device)),
            _ => Err(Error::NotClaimed),
        }
    }

    /// Closes the dialog's pairing; its token stops working and a pending
    /// claim is denied.
    pub(crate) fn cancel(&mut self, id: &PairingId) -> Result<()> {
        find(&mut self.open, id)?;
        self.close();
        Ok(())
    }

    fn close(&mut self) {
        if let Some(mut open) = self.open.take() {
            settle(&mut open, &mut self.outcomes, PairingState::Denied);
        }
    }
}

fn find<'a>(open: &'a mut Option<Open>, id: &PairingId) -> Result<&'a mut Open> {
    open.as_mut()
        .filter(|open| &open.id == id)
        .ok_or(Error::UnknownPairing)
}

/// Expires a token nobody claimed in time, or a claim nobody answered.
fn refresh(open: &mut Open, outcomes: &mut VecDeque<(ClaimId, PairingState)>, now_ms: u64) {
    match open.phase {
        Phase::Waiting if now_ms >= open.claimable_until() => {
            open.phase = Phase::Finished {
                claim: None,
                state: PairingState::Expired,
            };
        }
        Phase::Claimed { deadline_ms, .. } if now_ms >= deadline_ms => {
            settle(open, outcomes, PairingState::Expired);
        }
        _ => {}
    }
}

/// Finishes a claimed pairing with `state` and records the outcome for the
/// phone; anything else is left alone. Returns the phase it replaced.
fn settle(
    open: &mut Open,
    outcomes: &mut VecDeque<(ClaimId, PairingState)>,
    state: PairingState,
) -> Phase {
    let Phase::Claimed { claim, .. } = &open.phase else {
        return Phase::Waiting;
    };
    let claim = claim.clone();
    if outcomes.len() == REMEMBERED_OUTCOMES {
        outcomes.pop_front();
    }
    outcomes.push_back((claim.clone(), state.clone()));
    std::mem::replace(
        &mut open.phase,
        Phase::Finished {
            claim: Some(claim),
            state,
        },
    )
}

/// The six digits both screens show: the first four bytes of
/// SHA-256(token || public key || server nonce), modulo one million.
pub(crate) fn matching_code(token: &[u8], public_key: &[u8], nonce: &[u8]) -> String {
    let digest = sha256(&[token, public_key, nonce]);
    let value = u32::from_be_bytes([digest[0], digest[1], digest[2], digest[3]]);
    format!("{:06}", value % 1_000_000)
}
