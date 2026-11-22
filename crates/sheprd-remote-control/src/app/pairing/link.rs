//! The pairing link the dialog copies (`control::pairing_link`), so
//! another Sheprd on any network can pair through this computer's relays:
//! asked for once per click, sent when no other request is in flight, and
//! handed to the dialog once to put on the clipboard.

use crate::control::{self, ErrorCode};
use std::{
    fmt,
    time::{Duration, Instant},
};

/// A link's text, kept out of `Debug`: anyone holding it may claim.
#[derive(Clone, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct LinkText(pub String);

impl fmt::Debug for LinkText {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("LinkText(..)")
    }
}

/// Where the dialog's pairing link stands.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub enum Link {
    #[default]
    None,
    /// The person asked for one; `sent` once the request is on its way.
    Asked { sent: bool },
    /// The daemon's link, for the dialog to copy once.
    Ready { link: LinkText, lifetime: Duration },
    /// On the clipboard, claimable until `until`.
    Copied { until: Instant },
    /// This computer listens on no relay, so no link can reach it.
    NoRelays,
    /// Why the daemon made none, for the person.
    Failed { message: String },
}

impl Link {
    /// Whether a click asks for a link now: not while one is on its way.
    pub(super) fn ask(&mut self) {
        if !matches!(self, Self::Asked { .. }) {
            *self = Self::Asked { sent: false };
        }
    }

    /// Takes the daemon's answer to `PairLink`.
    pub(super) fn receive(&mut self, result: Result<control::Response, control::Error>) {
        *self = match result {
            Ok(control::Response::PairLink {
                link,
                expires_in_ms,
            }) => Self::Ready {
                link: LinkText(link),
                lifetime: Duration::from_millis(expires_in_ms),
            },
            Ok(_) => Self::Failed {
                message: crate::app::Error::Unexpected.to_string(),
            },
            Err(control::Error::Daemon {
                code: ErrorCode::NoRelays,
                ..
            }) => Self::NoRelays,
            Err(error) => Self::Failed {
                message: error.to_string(),
            },
        };
    }

    /// The link to copy, once; it then reads as copied until it expires.
    pub(super) fn take(&mut self, now: Instant) -> Option<String> {
        let Self::Ready { lifetime, .. } = self else {
            return None;
        };
        let until = now + *lifetime;
        match std::mem::replace(self, Self::Copied { until }) {
            Self::Ready { link, .. } => Some(link.0),
            _ => None,
        }
    }
}
