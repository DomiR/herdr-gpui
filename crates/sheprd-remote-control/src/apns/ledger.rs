//! How the last push of each kind went, for Sheprd's settings and
//! `sheprd remote push-status` (`Request::PushStatus`). Kept in memory
//! only: a restart, which every change of mode is, starts it empty. It
//! keeps no token, handle, device, or content.

use super::{Delivery, Sent};
use crate::control::push::{Last, Mode, Outcome, PushStatus};
use std::sync::{Mutex, PoisonError};

/// What a push was for.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Kind {
    /// A chat alert, or the phone's test push.
    Alert,
    LiveActivity,
}

#[derive(Debug, Default)]
pub(crate) struct Ledger(Mutex<Lasts>);

#[derive(Debug, Default)]
struct Lasts {
    alert: Option<Last>,
    live_activity: Option<Last>,
}

impl Ledger {
    /// Notes that a push of `kind` came to `sent` at `now_ms`.
    pub(crate) fn record(&self, kind: Kind, sent: &Sent, now_ms: u64) {
        let last = Some(Last {
            at_ms: now_ms,
            outcome: sent.into(),
        });
        let mut lasts = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        match kind {
            Kind::Alert => lasts.alert = last,
            Kind::LiveActivity => lasts.live_activity = last,
        }
    }

    /// The status `delivery` and the pushes so far make.
    pub(crate) fn status(&self, delivery: &Delivery) -> PushStatus {
        let lasts = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        PushStatus {
            mode: delivery.mode(),
            alert: lasts.alert.clone(),
            live_activity: lasts.live_activity.clone(),
        }
    }
}

impl Delivery {
    pub(crate) fn mode(&self) -> Mode {
        match self {
            Self::Off => Mode::Off,
            Self::Relay(_) => Mode::PushRelay,
        }
    }
}

impl From<&Sent> for Outcome {
    fn from(sent: &Sent) -> Self {
        match sent {
            Sent::Delivered => Self::Delivered,
            Sent::Gone => Self::Gone,
            Sent::Limited => Self::Limited,
            // The push relay gives no reason, only a status.
            Sent::Refused { status } => Self::Refused {
                status: *status,
                reason: None,
            },
            Sent::Unreachable => Self::Unreachable,
            Sent::Unsent => Self::Unsent,
        }
    }
}
