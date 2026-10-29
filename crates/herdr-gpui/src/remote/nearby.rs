//! Nearby pairing over Bluetooth, Herdr UI's side (`docs/ios-app-plan.md`,
//! "Bluetooth on the desktop"). While the pairing dialog shows an offer,
//! `herdr-nearby`, the Swift helper beside the executable on macOS,
//! advertises the nearby service and hands over each phone's `pair` frame;
//! the dialog relays it to the daemon's control socket and writes the
//! answer back. The helper holds no secrets and the code exchange runs in
//! the daemon: this only carries bytes. Without the helper (other
//! platforms, development builds) nothing starts, and the network and QR
//! paths remain.
//!
//! The helper speaks newline-delimited JSON on stdio (its header comment in
//! `macos/herdr-nearby/main.swift` lists the messages).
//!
//! With `[remote] presence` on, the helper also runs while remote control
//! does, advertising for paired phones' presence ([`presence`]). One
//! [`Station`] owns the one helper for both.

// keep-sorted start
#[cfg(target_os = "macos")]
mod error;
#[cfg(target_os = "macos")]
pub(crate) mod presence;
#[cfg(target_os = "macos")]
mod relay;
#[cfg(target_os = "macos")]
mod station;
// keep-sorted end

#[cfg(target_os = "macos")]
pub use error::Error;
#[cfg(target_os = "macos")]
pub(crate) use relay::*;
#[cfg(target_os = "macos")]
pub(crate) use station::{Advert, Station};

/// The Mac's Bluetooth radio, as the helper last reported it.
#[cfg(target_os = "macos")]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Radio {
    On,
    Off,
    /// Herdr is not allowed to use Bluetooth (System Settings > Privacy).
    Unauthorized,
    Unsupported,
    /// Resetting or not known yet.
    Unknown,
}
