//! Joining the fleet over Bluetooth in one step (`docs/relay-plan.md`,
//! Adding a nearby Sheprd from another network, and Fleet, Joining).
//!
//! Two computers that pair over Bluetooth leave as members of one fleet,
//! whichever of them was in it: the computer with Pair device open names
//! its fleet in the sealed offer ([`Hint`]); the computer that claims
//! either invites it into its own fleet ([`ClaimFleet::Invite`]) or asks
//! to join the one it named ([`ClaimFleet::Join`]). The joiner's bundle
//! travels with the claim, or with the approved answer, as a [`Joining`].
//!
//! A [`Joining`] carries two signatures by the joiner's fleet key: the
//! join signature every member checks in the `Admit` (`Bundle::join_message`),
//! and a binding over the pairing's token, which only the two computers
//! that compared the code hold. A bundle copied from another join, or a
//! claim replayed from another pairing, fails the binding, so no claim
//! admits a key its sender did not hold for this very pairing.

use super::{Bundle, DOMAIN, Error, FleetId, Kind, MAX_OP, Result, sha256, sync};
use p256::ecdsa::{
    Signature, SigningKey, VerifyingKey,
    signature::{Signer as _, Verifier as _},
};
use serde::{Deserialize, Serialize};

/// What the binding over a pairing's token says, after [`DOMAIN`].
const BINDING: &[u8] = b"/bluetooth-join";
/// What a pairing's token is hashed with before it is bound.
const PAIRING: &[u8] = b"/bluetooth-pairing";
/// The longest bundle a [`Joining`] carries, base64url.
const MAX_BUNDLE_TEXT: usize = MAX_OP.div_ceil(3) * 4;
/// A signature, base64url.
const SIGNATURE_TEXT: usize = 86;

/// A computer's fleet, as it names it to a computer pairing with it: only
/// a live computer member names one.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Hint {
    /// The fleet's id, hex.
    pub fleet: String,
    /// Its computer members, for the dialog's words; never trusted.
    pub computers: u16,
    /// The hosts it shares, members' own not counted; never trusted.
    pub hosts: u16,
}

impl Hint {
    /// Whether the id is a fleet id, as anything read from the other
    /// computer must be before it is shown or acted on.
    #[must_use]
    pub fn valid(&self) -> bool {
        FleetId::from_hex(&self.fleet).is_some()
    }

    /// The fleet it names.
    pub fn id(&self) -> Result<FleetId> {
        sync::fleet(&self.fleet)
    }
}

/// What a claim asks of the fleet, beside pairing.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClaimFleet {
    /// The claiming computer is in this fleet and invites the other one:
    /// its person's Allow signs the other computer's bundle for it.
    Invite(Hint),
    /// The claiming computer joins the fleet the other one named: its
    /// person's Allow admits this bundle.
    Join(Joining),
}

/// What a claim asked of the fleet, as the pairing dialog shows it.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Asked {
    /// The claiming computer asks to join `fleet`, this computer's own,
    /// with keys checked to be the ones it claims with.
    Join { fleet: String },
    /// The claiming computer invites this one into its fleet.
    Invite(Hint),
}

/// A joiner's bundle, for the fleet it names, bound to one pairing.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Joining {
    /// `Bundle::to_bytes`, base64url.
    pub bundle: String,
    /// Over `Bundle::join_message`, base64url: what the `Admit` carries.
    pub signature: String,
    /// Over [`binding_message`], base64url.
    pub binding: String,
}

/// What an approved claim says about the fleet.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovedFleet {
    /// The claiming computer was admitted to this fleet: it takes the log
    /// from the other computer over the SSH the pairing opened.
    Joined { fleet: String },
    /// The other computer agreed to join the claimant's fleet: the
    /// claimant admits this bundle and hands the log over.
    Joining(Joining),
}

/// The token of a pairing, hashed for binding: what both computers that
/// compared the code hold, and nothing else does.
#[must_use]
pub fn pairing_digest(token: &[u8; 32]) -> [u8; 32] {
    sha256(&[DOMAIN, PAIRING, token])
}

/// What the joiner signs to bind `bundle` for `fleet` to the pairing whose
/// token hashes to `pairing`.
pub fn binding_message(fleet: &FleetId, pairing: &[u8; 32], bundle: &Bundle) -> Result<Vec<u8>> {
    let mut message = Vec::with_capacity(DOMAIN.len() + BINDING.len() + 64 + MAX_OP);
    message.extend_from_slice(DOMAIN);
    message.extend_from_slice(BINDING);
    message.extend_from_slice(&fleet.0);
    message.extend_from_slice(pairing);
    message.extend_from_slice(&bundle.to_bytes()?);
    Ok(message)
}

/// Signs the binding of `bundle` with `key`, its own fleet key.
pub fn bind(
    key: &SigningKey,
    fleet: &FleetId,
    pairing: &[u8; 32],
    bundle: &Bundle,
) -> Result<String> {
    let signature: Signature = key.sign(&binding_message(fleet, pairing, bundle)?);
    Ok(sync::text(&signature.to_bytes()))
}

/// Signs `bundle` for `fleet` and binds it to `pairing` with `key`, its
/// own fleet key: a joiner's whole [`Joining`].
pub fn joining(
    key: &SigningKey,
    fleet: &FleetId,
    pairing: &[u8; 32],
    bundle: &Bundle,
) -> Result<Joining> {
    let message = bundle.join_message(fleet).ok_or(Error::Invalid)?;
    let signature: Signature = key.sign(&message);
    Ok(Joining {
        bundle: sync::text(&bundle.to_bytes()?),
        signature: sync::text(&signature.to_bytes()),
        binding: bind(key, fleet, pairing, bundle)?,
    })
}

impl Joining {
    /// The computer's bundle, once both signatures check against its own
    /// key for `fleet` and the pairing whose token hashes to `pairing`.
    /// A phone's bundle, a malformed one, or one over its bounds is
    /// refused.
    pub fn check(&self, fleet: &FleetId, pairing: &[u8; 32]) -> Result<Bundle> {
        if self.bundle.len() > MAX_BUNDLE_TEXT
            || self.signature.len() > SIGNATURE_TEXT
            || self.binding.len() > SIGNATURE_TEXT
        {
            return Err(Error::Bounds);
        }
        let bundle = Bundle::from_bytes(&sync::bytes(&self.bundle)?)?;
        if bundle.kind != Kind::Computer {
            return Err(Error::Invalid);
        }
        let key = VerifyingKey::from_sec1_bytes(&bundle.key).map_err(|_| Error::Signature)?;
        let join = bundle.join_message(fleet).ok_or(Error::Invalid)?;
        verify(&key, &join, &self.signature).map_err(|_| Error::JoinSignature)?;
        verify(
            &key,
            &binding_message(fleet, pairing, &bundle)?,
            &self.binding,
        )?;
        Ok(bundle)
    }
}

fn verify(key: &VerifyingKey, message: &[u8], signature: &str) -> Result<()> {
    let signature =
        Signature::from_slice(&sync::bytes(signature)?).map_err(|_| Error::Signature)?;
    key.verify(message, &signature)
        .map_err(|_| Error::Signature)
}
