//! The Apple Push Notification service, from the provider's side: the
//! credentials, the ES256 provider token, the HTTP/2 sender, and the sealing
//! that keeps a push's content from APNs. The push relay sends with the
//! app publisher's key, the only one there is; the daemon only seals pushes
//! and hands them to the relay. What a push says stays with them.
//!
//! - [`Config`]: the push relay's APNs key (`DIR/apns/AuthKey.p8`) and
//!   `config.json`.
//! - [`ProviderToken`]: the JWT APNs authenticates a provider by, refreshed
//!   every fifty minutes.
//! - [`Sender`]: the push relay's HTTP/2 request to `api.push.apple.com` or
//!   the sandbox.
//! - [`RelayClient`]: the daemon's request to the push relay. It takes only
//!   a [`SealedPush`].
//! - [`seal`]: sealing to a P-256 key, and opening with its private half.
//! - [`DeviceToken`]: a checked push token.
//!
//! The host is one of Apple's two, chosen by the [`Config`], or one of the
//! two relays compiled in as [`RelayHost`], so nothing a device or a caller
//! supplies can make a sender post anywhere else.

mod config;
mod error;
mod jwt;
mod relay;
pub mod seal;
mod sealed;
mod send;
mod token;

pub use config::{Config, dir, is_private, load_secret_key};
pub use error::{Error, Result};
pub use jwt::ProviderToken;
pub use relay::{RelayClient, RelayHost, RelayOutcome, push_body};
pub use sealed::{SealedPush, Sealer};
pub use send::{Host, MAX_PAYLOAD, Outcome, Priority, Push, PushType, Sender};
pub use token::DeviceToken;

#[cfg(test)]
mod tests;
