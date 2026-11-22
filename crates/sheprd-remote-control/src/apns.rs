//! Live Activities and notifications on paired iPhones through the Apple
//! Push Notification service: the companion app in `ios/` shows one
//! activity per running chat on the lock screen and in the Dynamic Island,
//! which the daemon starts, updates, and ends with `liveactivity` pushes,
//! and chat notifications sealed to the device.
//!
//! The credentials, the HTTP/2 sender, the relay client, and the sealing
//! live in `sheprd_apns`. [`Delivery`] picks how pushes leave: through the
//! push relay when `[remote] push_relay` allows it
//! (`docs/apns-relay-plan.md`), else not at all. The daemon has no APNs key
//! of its own and never posts to Apple: the iOS app has one publisher, and
//! only the push relay holds the key APNs takes for it.
//!
//! - [`decide`]: pure, from the same inputs as [`crate::push::decide`],
//!   whether a chat change starts, updates, or ends the chat's activity.
//! - `redact`: what one device's activity may say, from the computer's and
//!   the phone's choices, applied just before sending.
//! - [`Pacer`]: holds back step updates so a busy turn does not spend the
//!   phone's Live Activity budget.
//! - [`Registration`]: the push tokens one device sent, kept in the device
//!   store's opaque slot as JSON; each a relay handle or, from an older
//!   app, a plain token, which is kept but never sent to ([`Recipient`]).
//! - [`alert`]: chat notifications for the native app, sealed to the
//!   device with [`seal`] so the relay and APNs carry only ciphertext.
//! - `relayed`: Live Activity payloads sealed the same way, for the relay.
//! - [`push_relay_revoke`]: an unpaired device's push relay handles,
//!   revoked at the push relay.
//! - [`Ledger`]: how the last push of each kind went, for Sheprd.
//!
//! A device only ever supplies hex tokens, or relay handles with the name
//! of an environment. The host is one of the two relays compiled in,
//! `push.sheprd.org` and `push-sandbox.sheprd.org` for Xcode builds, chosen
//! by the handle's environment, so a paired device cannot make the daemon
//! post anywhere else.

pub(crate) mod alert;
mod decide;
mod delivery;
mod ledger;
mod pacer;
mod payload;
pub(crate) mod push_relay_revoke;
mod recipient;
mod redact;
mod relayed;
mod terminal;
mod tokens;

pub(crate) use decide::{computer_id, decide};
#[cfg(test)]
pub(crate) use delivery::Relays;
pub(crate) use delivery::{Delivery, Sent};
pub(crate) use ledger::{Kind, Ledger};
pub(crate) use pacer::Pacer;
#[cfg(test)]
pub(crate) use payload::ATTRIBUTES_TYPE;
pub(crate) use payload::{Alert, Attributes, ContentState, Event, Live, Phase};
pub(crate) use recipient::Handle;
pub(crate) use recipient::Recipient;
pub(crate) use sheprd_apns::{DeviceToken, Priority, Push, PushType, seal};
pub(crate) use terminal::decide as decide_terminal;
#[cfg(test)]
pub(crate) use tokens::ActivityToken;
pub(crate) use tokens::{Registration, Update};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    // keep-sorted start
    #[error(transparent)]
    Apns(#[from] sheprd_apns::Error),
    #[error("a push relay handle must be a key id and sealed base64url, at most 2048 bytes")]
    Handle,
    #[error("could not encode the APNs payload")]
    Json(#[from] serde_json::Error),
    #[error("the notification key is not a P-256 point in base64url")]
    NotifyKey,
    #[error("a Live Activity's pane must be 1 to 128 characters with no control characters")]
    Pane,
    #[error("the APNs payload is {len} bytes, more than {max}", max = payload::MAX_PAYLOAD)]
    PayloadTooLarge { len: usize },
    #[error("{len} Live Activity tokens, more than {max}", max = tokens::MAX_ACTIVITIES)]
    TooManyActivities { len: usize },
    // keep-sorted end
}

pub(crate) type Result<T> = std::result::Result<T, Error>;

#[cfg(test)]
mod tests;
