//! How this daemon's APNs pushes leave it: through the push relay
//! (`docs/apns-relay-plan.md`), or not at all. The daemon holds no APNs key
//! and never contacts Apple itself: only the push relay, run by the iOS
//! app's publisher, holds the key APNs takes for the app.
//!
//! The relay reaches only handles. A plain token, which an older app still
//! sends, is kept with the device but reaches nothing: that app gets no
//! APNs push. The native app has no Web Push subscription of its own; only
//! a browser app the user installed and subscribed gets Web Push. Every
//! payload is sealed to the phone ([`SealedPush`]),
//! Live Activities included.
//!
//! A relay outage must not stall the notifier: once a relay could not be
//! reached, pushes through it are dropped at once for [`OUTAGE_MS`], so
//! alerts fall back to Web Push and activity updates are not queued.

use super::{
    Live, Recipient, Result,
    alert::{self, Sealed},
    recipient::Handle,
    seal,
};
use p256::PublicKey;
use sheprd_apns::{RelayClient, RelayHost, RelayOutcome, SealedPush};
use std::sync::atomic::{AtomicU64, Ordering};

/// How long pushes through the relay are dropped after it could not be
/// reached, each try having already waited through its retries.
pub(crate) const OUTAGE_MS: u64 = 60 * 1000;

/// What came of one push, as the caller acts on it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum Sent {
    Delivered,
    /// The handle will never work again: forget it.
    Gone,
    /// Too many pushes for this phone for now.
    Limited,
    /// Refused. The relay gives no reason, and its status stands for one:
    /// 400 for a push it could not read, 403 for a handle it no longer
    /// takes, which only the phone's next registration replaces.
    Refused {
        status: u16,
    },
    /// The relay could not be reached, or it is out and the push was
    /// dropped.
    Unreachable,
    /// Nothing went out: the push could not be built or sealed, or the
    /// recipient is a plain token, which the relay cannot reach.
    Unsent,
}

impl From<RelayOutcome> for Sent {
    fn from(outcome: RelayOutcome) -> Self {
        let refused = |status| Self::Refused { status };
        match outcome {
            RelayOutcome::Delivered => Self::Delivered,
            RelayOutcome::Gone => Self::Gone,
            RelayOutcome::Limited => Self::Limited,
            RelayOutcome::Invalid => refused(400),
            RelayOutcome::Stale => refused(403),
            RelayOutcome::Unavailable => Self::Unreachable,
        }
    }
}

pub(crate) enum Delivery {
    /// The push relay is off: chat changes send no APNs push.
    Off,
    Relay(Box<Relays>),
}

impl Delivery {
    /// The push relay when `push_relay` allows it, else nothing. Reads no
    /// file: an APNs key left in the state directory is not used.
    pub(crate) fn new(push_relay: bool) -> Result<Self> {
        Ok(if push_relay {
            Self::Relay(Box::new(Relays::new()?))
        } else {
            Self::Off
        })
    }

    /// The push relays, when pushes go through them.
    pub(crate) fn push_relays(&self) -> Option<&Relays> {
        match self {
            Self::Relay(relays) => Some(relays),
            Self::Off => None,
        }
    }

    pub(crate) fn is_off(&self) -> bool {
        matches!(self, Self::Off)
    }

    /// As the log names it.
    pub(crate) fn name(&self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Relay(_) => "push relay",
        }
    }

    /// As `POST /api/apns` tells the phone, so its Settings can say how
    /// this computer delivers: `None` when it sends nothing. An older
    /// daemon could also say `own_key`, which the phone reads as unknown.
    pub(crate) fn wire_name(&self) -> Option<&'static str> {
        match self {
            Self::Off => None,
            Self::Relay(_) => Some("push_relay"),
        }
    }

    /// Whether this mode can send to `recipient`: only the relay, and only
    /// to a handle. A plain token is never sent to.
    pub(crate) fn reaches(&self, recipient: &Recipient) -> bool {
        matches!((self, recipient), (Self::Relay(_), Recipient::Handle(_)))
    }

    /// Sends `sealed`, sealed to `key`, to `recipient`, collapsing under
    /// `tag` in the pairing's scope (`alert::scope`), at `now_ms`.
    pub(crate) async fn alert(
        &self,
        recipient: &Recipient,
        key: &PublicKey,
        sealed: Sealed,
        tag: &str,
        now_ms: u64,
    ) -> Sent {
        let scope = alert::scope(key);
        match (self, recipient) {
            (Self::Relay(relays), Recipient::Handle(handle)) => {
                if relays.is_down(handle.environment(), now_ms) {
                    return Sent::Unreachable;
                }
                let clear = alert::clear(&scope, &sealed.pane, tag);
                let Ok(push) = SealedPush::build(key, seal::INFO, &clear, |sealer| {
                    alert::push(sealed, &scope, tag, |plaintext| Ok(sealer.seal(plaintext)?))
                }) else {
                    return Sent::Unsent;
                };
                relays.post(handle, &push, now_ms).await
            }
            _ => Sent::Unsent,
        }
    }

    /// Sends `live` to `recipient` at `now_ms`. Through the relay it is
    /// sealed to `key`, the phone's notify key, and goes nowhere without
    /// one; an update during a relay outage is dropped.
    pub(crate) async fn live(
        &self,
        recipient: &Recipient,
        key: Option<&PublicKey>,
        live: &Live,
        now_ms: u64,
    ) -> Sent {
        let now_s = now_ms / 1000;
        match (self, recipient) {
            (Self::Relay(relays), Recipient::Handle(handle)) => {
                let Some(key) = key else {
                    return Sent::Unsent;
                };
                if relays.is_down(handle.environment(), now_ms) {
                    return Sent::Unreachable;
                }
                let Ok(push) = live.sealed_push(key, now_s) else {
                    return Sent::Unsent;
                };
                relays.post(handle, &push, now_ms).await
            }
            _ => Sent::Unsent,
        }
    }
}

/// The two relays, one per APNs environment. Each keeps its own outage, so
/// a sandbox relay that is down never holds back production pushes.
pub(crate) struct Relays {
    production: Relay,
    sandbox: Relay,
}

struct Relay {
    client: RelayClient,
    /// Until when pushes are dropped, in ms since the Unix epoch.
    down_until_ms: AtomicU64,
}

impl Relay {
    fn new(host: RelayHost) -> sheprd_apns::Result<Self> {
        Ok(Self {
            client: RelayClient::new(host)?,
            down_until_ms: AtomicU64::new(0),
        })
    }
}

impl Relays {
    /// Opens no connection until a push goes out.
    pub(crate) fn new() -> sheprd_apns::Result<Self> {
        Ok(Self {
            production: Relay::new(RelayHost::Production)?,
            sandbox: Relay::new(RelayHost::Sandbox)?,
        })
    }

    fn relay(&self, host: RelayHost) -> &Relay {
        match host {
            RelayHost::Production => &self.production,
            RelayHost::Sandbox => &self.sandbox,
        }
    }

    pub(crate) fn is_down(&self, host: RelayHost, now_ms: u64) -> bool {
        now_ms < self.relay(host).down_until_ms.load(Ordering::Relaxed)
    }

    /// Drops pushes through `host` for [`OUTAGE_MS`] from `now_ms`.
    pub(crate) fn mark_down(&self, host: RelayHost, now_ms: u64) {
        self.relay(host)
            .down_until_ms
            .fetch_max(now_ms.saturating_add(OUTAGE_MS), Ordering::Relaxed);
    }

    /// Asks the push relay `handle` names to refuse it from now on, or
    /// gives up at once while that push relay is out: revoking is best effort, and the
    /// handle's credential runs out on its own.
    pub(crate) async fn revoke(&self, handle: &Handle, now_ms: u64) -> RelayOutcome {
        let host = handle.environment();
        if self.is_down(host, now_ms) {
            return RelayOutcome::Unavailable;
        }
        let outcome = self.relay(host).client.revoke(handle.secret()).await;
        if outcome == RelayOutcome::Unavailable {
            self.mark_down(host, now_ms);
        }
        outcome
    }

    /// Sends `push` to the relay `handle` names, or drops it while that
    /// relay is out.
    async fn post(&self, handle: &Handle, push: &SealedPush, now_ms: u64) -> Sent {
        let host = handle.environment();
        if self.is_down(host, now_ms) {
            return Sent::Unreachable;
        }
        let outcome = self.relay(host).client.post(handle.secret(), push).await;
        if outcome == RelayOutcome::Unavailable {
            self.mark_down(host, now_ms);
        }
        outcome.into()
    }
}
