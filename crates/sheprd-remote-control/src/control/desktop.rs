//! `sheprd-remote-control desktop-pair`: what Sheprd and the child say to
//! each other on stdio while this computer adds a nearby Sheprd over
//! Bluetooth (`docs/relay-plan.md`, Adding a nearby Sheprd from another
//! network). One JSON object per line each way.
//!
//! Sheprd carries Bluetooth: it writes each [`PairOutput::Frame`] to the
//! other computer's `pair` characteristic and hands back what that computer
//! answers as [`PairInput::Frame`]. The child does everything else: the code
//! exchange, the claim through the other computer's relays, and saving the
//! machine. Closing its input cancels.
//!
//! Run with `--link`, it adds a Sheprd from a pairing link instead
//! (`super::pairing_link`): its first input is [`PairInput::Link`], no
//! frame is carried, it answers [`PairOutput::Linked`] in place of
//! [`PairOutput::Code`], and the code both screens show comes with
//! [`PairOutput::Claimed`].

use super::fleet::bluetooth::{ApprovedFleet, ClaimFleet, Hint};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde::{Deserialize, Serialize};

/// The largest frame either side carries: what the Bluetooth helper takes.
pub const MAX_FRAME: usize = 4096;
/// The longest line either side writes: a base64 frame and its envelope,
/// or a fleet bundle (at most `fleet::MAX_OP`) and its two signatures.
pub const MAX_LINE: usize = 32 * 1024;

/// From Sheprd to the child.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum PairInput {
    /// What the other computer answered over Bluetooth, base64.
    Frame { data: String },
    /// What the claim asks of the fleet, decided once both screens show
    /// the code: the child waits for it before claiming. `None` pairs
    /// only.
    Claim {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        fleet: Option<ClaimFleet>,
    },
    /// The pairing link the person pasted, with `--link`: never on the
    /// command line, where other users could read it.
    Link { link: String },
}

/// From the child to Sheprd.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum PairOutput {
    /// A message for the other computer over Bluetooth, base64.
    Frame { data: String },
    /// The code both screens show, and the other computer's name. The
    /// child then waits for [`PairInput::Claim`].
    Code {
        code: String,
        computer: String,
        /// The other computer's fleet, when it is a member of one.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        fleet: Option<Hint>,
        /// The pairing's token, hashed (`bluetooth::pairing_digest`),
        /// base64url: what a joiner's bundle is bound to (`Ask::Joining`).
        pairing: String,
    },
    /// What a pairing link names, with `--link`: the child then waits for
    /// [`PairInput::Claim`], as after [`PairOutput::Code`]; the code comes
    /// once the other computer takes the claim.
    Linked {
        computer: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        fleet: Option<Hint>,
        /// As [`PairOutput::Code`]'s.
        pairing: String,
    },
    /// The claim reached the other computer through a relay; its user
    /// compares codes and answers there. `code` is the one it shows, which
    /// a claim from a pairing link learns only now.
    Claimed {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        code: Option<String>,
    },
    /// Allowed: the machine is saved and reachable as `target`.
    Paired {
        target: String,
        label: String,
        /// What became of the fleet, checked by the child: this computer
        /// joined the other's (`Joined`), or the other computer's bundle,
        /// bound to this pairing, for this computer to admit (`Joining`).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        fleet: Option<ApprovedFleet>,
    },
    /// Why pairing stopped, for the person.
    Failed { message: String },
}

impl PairInput {
    pub fn frame(data: &[u8]) -> Self {
        Self::Frame {
            data: STANDARD.encode(data),
        }
    }
}

impl PairOutput {
    pub fn frame(data: &[u8]) -> Self {
        Self::Frame {
            data: STANDARD.encode(data),
        }
    }
}

/// A frame's bytes, when it is base64 of at most [`MAX_FRAME`] bytes.
pub fn frame_bytes(data: &str) -> Option<Vec<u8>> {
    let bytes = STANDARD.decode(data).ok()?;
    (bytes.len() <= MAX_FRAME).then_some(bytes)
}
