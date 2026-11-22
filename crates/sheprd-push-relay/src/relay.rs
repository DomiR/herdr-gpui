//! The relay's decisions: every check a push or revocation passes, in order,
//! and the reply each gives. Pure of HTTP, with APNs behind [`Gateway`] so
//! tests can stand in for it.

use crate::{
    attest::{self, Environment, KeyId, Verifier},
    credential::{Credential, DeviceId, Issuer, Spent},
    handle::{Handle, Keyring},
    limit::{Limiter, Rates, source_key},
    metrics::{Attestation, Challenge, Limit, Metrics},
    store::{Memory, Revoked, Store},
};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
use sheprd_apns::{DeviceToken, Host, MAX_PAYLOAD, Outcome, Priority, Push, PushType, Sender};
use sheprd_dashboard::labels;
use std::{
    future::Future,
    net::IpAddr,
    sync::{Arc, Mutex, PoisonError},
};

/// APNs's longest `apns-collapse-id`.
const MAX_COLLAPSE_ID: usize = 64;

/// Where pushes go: APNs, or a stand-in in tests.
pub trait Gateway: Send + Sync + 'static {
    fn post(
        &self,
        token: &DeviceToken,
        push: &Push,
        now_s: u64,
    ) -> impl Future<Output = sheprd_apns::Result<Outcome>> + Send;
}

impl Gateway for Sender {
    async fn post(
        &self,
        token: &DeviceToken,
        push: &Push,
        now_s: u64,
    ) -> sheprd_apns::Result<Outcome> {
        Sender::post(self, token, push, now_s).await
    }
}

/// `POST /v1/push`. It has no topic or host: the relay sets both.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PushRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub collapse_id: Option<String>,
    pub handle: SecretString,
    /// The APNs payload as JSON text.
    pub payload: String,
    pub priority: Priority,
    pub push_type: PushType,
}

/// `POST /v1/revoke`.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RevokeRequest {
    pub handle: SecretString,
}

/// `POST /v1/attest`, as `DCAppAttestService` gives its parts: the key id
/// and attestation in standard base64, and the challenge as the relay sent
/// it, whose SHA-256 is the attestation's client data hash.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AttestRequest {
    pub attestation: String,
    pub challenge: String,
    pub key_id: String,
}

/// What an attested phone gets: a credential for its handles, and the key
/// to seal them to.
#[derive(Debug, Eq, PartialEq, Serialize)]
pub struct Issued {
    pub credential: String,
    pub expires_s: u64,
    /// The id handles name the key by, before their `.`.
    pub relay_key_id: String,
    /// The relay's public key, an uncompressed SEC1 point in base64url.
    pub relay_key: String,
}

/// The longest attestation body field: an attestation is about 5,900 bytes,
/// 7,900 in base64.
const MAX_ATTESTATION: usize = 12 * 1024;

labels! {
    /// What a caller is told. `Refused` covers every bad handle alike, so a
    /// caller learns nothing about why. Each label is its wire name too.
    #[derive(Serialize)]
    #[serde(rename_all = "snake_case")]
    Reply {
        Delivered => "delivered",
        /// The credential ran out: the phone must renew it. Keep the
        /// handle's place, since a new one will come.
        Expired => "expired",
        /// The handle will never work again: the daemon forgets it.
        Gone => "gone",
        /// The request is malformed.
        Invalid => "invalid",
        Limited => "limited",
        Refused => "refused",
        Revoked => "revoked",
        /// The relay or APNs cannot take it now. Not worth queueing.
        Unavailable => "unavailable",
    }
}

struct Limits {
    attest: Limiter<IpAddr>,
    device: Limiter<DeviceId>,
    handle: Limiter<String>,
    revoke: Limiter<DeviceId>,
    source: Limiter<IpAddr>,
    spent: Spent,
}

/// One relay instance serves one APNs environment.
pub struct Relay<G, S = Memory> {
    attestations: Verifier,
    environment: Host,
    gateway: G,
    issuer: Issuer,
    keys: Keyring,
    limits: Mutex<Limits>,
    metrics: Arc<Metrics>,
    store: S,
}

impl<G: Gateway, S: Store> Relay<G, S> {
    #[must_use]
    pub fn new(
        gateway: G,
        environment: Host,
        issuer: Issuer,
        keys: Keyring,
        store: S,
        rates: Rates,
        attestations: Verifier,
    ) -> Self {
        Self {
            attestations,
            environment,
            gateway,
            issuer,
            keys,
            limits: Mutex::new(Limits {
                attest: Limiter::new(rates.attest),
                device: Limiter::new(rates.device),
                handle: Limiter::new(rates.handle),
                revoke: Limiter::new(rates.revoke),
                source: Limiter::new(rates.source),
                spent: Spent::default(),
            }),
            metrics: Arc::default(),
            store,
        }
    }

    fn limits(&self) -> std::sync::MutexGuard<'_, Limits> {
        self.limits.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// What the relay has counted, for `/metrics` and the dashboard.
    #[must_use]
    pub fn metrics(&self) -> &Arc<Metrics> {
        &self.metrics
    }

    /// The APNs environment this relay serves.
    #[must_use]
    pub fn environment(&self) -> Host {
        self.environment
    }

    /// Whether a limit refused the request, counting the one that did.
    fn refused(&self, limit: Option<Limit>) -> bool {
        limit
            .inspect(|limit| self.metrics.limited(*limit))
            .is_some()
    }

    /// Before any cryptography, so a flood of junk costs no key agreement.
    pub fn source_allowed(&self, source: IpAddr, now_ms: u64) -> bool {
        let allowed = self.limits().source.allow(source_key(source), now_ms);
        !self.refused((!allowed).then_some(Limit::Source))
    }

    /// Whether `source` may ask for a challenge or attest now. Both spend
    /// the general limit too.
    fn attest_allowed(&self, source: IpAddr, now_ms: u64) -> bool {
        let refused = {
            let mut limits = self.limits();
            let key = source_key(source);
            if !limits.source.allow(key, now_ms) {
                Some(Limit::Source)
            } else if !limits.attest.allow(key, now_ms) {
                Some(Limit::Attest)
            } else {
                None
            }
        };
        !self.refused(refused)
    }

    /// A challenge for a phone at `source` to attest over.
    pub fn challenge(&self, source: IpAddr, now_ms: u64) -> Result<String, Reply> {
        let challenge = if self.attest_allowed(source, now_ms) {
            self.issuer.challenge(now_ms / 1000).map_err(|error| {
                tracing::error!(%error, "could not make a challenge");
                Reply::Unavailable
            })
        } else {
            Err(Reply::Limited)
        };
        self.metrics.challenge(match challenge {
            Ok(_) => Challenge::Issued,
            Err(reply) => Challenge::from(reply),
        });
        challenge
    }

    /// A credential for a key App Attest vouches for, made over one of this
    /// relay's challenges, once, by a build for this relay's APNs
    /// environment: Xcode builds attest in development and push through the
    /// sandbox.
    pub fn attest(
        &self,
        source: IpAddr,
        request: &AttestRequest,
        now_ms: u64,
    ) -> Result<Issued, Reply> {
        let issued = self.attested(source, request, now_ms);
        self.metrics.attestation(match issued {
            Ok(_) => Attestation::Issued,
            Err(reply) => Attestation::from(reply),
        });
        issued
    }

    fn attested(
        &self,
        source: IpAddr,
        request: &AttestRequest,
        now_ms: u64,
    ) -> Result<Issued, Reply> {
        if !self.attest_allowed(source, now_ms) {
            return Err(Reply::Limited);
        }
        let now_s = now_ms / 1000;
        if request.attestation.len() > MAX_ATTESTATION {
            return Err(Reply::Invalid);
        }
        let (Ok(attestation), Ok(key_id)) = (
            STANDARD.decode(&request.attestation),
            STANDARD.decode(&request.key_id),
        ) else {
            return Err(Reply::Invalid);
        };
        let Ok(key_id) = KeyId::try_from(key_id) else {
            return Err(Reply::Invalid);
        };
        if !self.issuer.check_challenge(&request.challenge, now_s) {
            return Err(Reply::Refused);
        }
        let client_data_hash: [u8; 32] = Sha256::digest(&request.challenge).into();
        let attested = self
            .attestations
            .verify(&attestation, &key_id, &client_data_hash, now_s)
            .map_err(|error: attest::Error| {
                // A fixed reason, safe to log: no key, token, or address.
                tracing::info!(%error, "refused an attestation");
                Reply::Refused
            })?;
        let expected = match self.environment {
            Host::Production => Environment::Production,
            Host::Sandbox => Environment::Development,
        };
        if attested.environment != expected {
            return Err(Reply::Refused);
        }
        // Last, once the attestation is good, so a bad one spends nothing.
        if !self.limits().spent.spend(&request.challenge, now_s) {
            return Err(Reply::Refused);
        }
        let (relay_key_id, relay_key) = self.keys.current().ok_or(Reply::Unavailable)?;
        let device = self.issuer.device(&key_id);
        self.metrics.attested(&device);
        Ok(Issued {
            credential: self.issuer.issue(device, self.environment, now_s),
            expires_s: now_s + crate::credential::LIFETIME_S,
            relay_key_id,
            relay_key,
        })
    }

    /// The opened handle and its credential, or the reply that ends the
    /// request. A revoked handle is checked by [`Self::push`] alone: revoking
    /// one twice should not fail.
    fn opened(&self, text: &str) -> Result<(Handle, Credential), Reply> {
        let handle = self.keys.open(text).ok_or(Reply::Refused)?;
        let credential = self
            .issuer
            .check(handle.credential.expose_secret())
            .filter(|credential| credential.environment == self.environment)
            .ok_or(Reply::Refused)?;
        Ok((handle, credential))
    }

    /// Sends one push, at `now_ms`, for a caller at `source`.
    pub async fn push(&self, source: IpAddr, request: PushRequest, now_ms: u64) -> Reply {
        self.metrics.kind(request.push_type);
        let reply = self.send(source, request, now_ms).await;
        self.metrics.push(reply);
        reply
    }

    async fn send(&self, source: IpAddr, request: PushRequest, now_ms: u64) -> Reply {
        if !self.source_allowed(source, now_ms) {
            return Reply::Limited;
        }
        let (handle, credential) = match self.opened(request.handle.expose_secret()) {
            Ok(opened) => opened,
            Err(reply) => return reply,
        };
        let now_s = now_ms / 1000;
        if credential.expires_s <= now_s {
            return Reply::Expired;
        }
        match self.store.is_revoked(credential.device, &handle.id).await {
            Ok(false) => {}
            Ok(true) => return Reply::Gone,
            // A revocation that cannot be checked may be in force.
            Err(error) => {
                tracing::error!(%error, "could not check revocations");
                return Reply::Unavailable;
            }
        }
        if !handle.kind.allows(request.push_type) {
            return Reply::Refused;
        }
        if !valid_payload(&request.payload) || !valid_collapse_id(request.collapse_id.as_deref()) {
            return Reply::Invalid;
        }
        let refused = {
            let mut limits = self.limits();
            if !limits.device.allow(credential.device, now_ms) {
                Some(Limit::Device)
            } else if !limits.handle.allow(handle.id, now_ms) {
                Some(Limit::Handle)
            } else {
                None
            }
        };
        if self.refused(refused) {
            return Reply::Limited;
        }
        self.metrics.pushed(&credential.device, now_s);
        let push = Push {
            body: request.payload.into_bytes(),
            collapse_id: request.collapse_id,
            priority: request.priority,
            push_type: request.push_type,
        };
        match self.gateway.post(&handle.token, &push, now_s).await {
            Ok(Outcome::RateLimited) => {
                self.metrics.limited(Limit::Apns);
                Reply::Limited
            }
            Ok(outcome) => reply(outcome),
            Err(error) => {
                tracing::warn!(%error, "could not reach APNs");
                Reply::Unavailable
            }
        }
    }

    /// Revokes one handle, for its daemon when a phone unpairs. Holding the
    /// handle is the authority: only that daemon does.
    pub async fn revoke(&self, source: IpAddr, request: RevokeRequest, now_ms: u64) -> Reply {
        let reply = self.revoke_handle(source, request, now_ms).await;
        self.metrics.revocation(reply);
        reply
    }

    async fn revoke_handle(&self, source: IpAddr, request: RevokeRequest, now_ms: u64) -> Reply {
        if !self.source_allowed(source, now_ms) {
            return Reply::Limited;
        }
        let (handle, credential) = match self.opened(request.handle.expose_secret()) {
            Ok(opened) => opened,
            Err(reply) => return reply,
        };
        // Nothing to keep: the relay refuses this credential already.
        if credential.expires_s <= now_ms / 1000 {
            return Reply::Revoked;
        }
        let refused = {
            let mut limits = self.limits();
            if !limits.device.allow(credential.device, now_ms) {
                Some(Limit::Device)
            } else if !limits.revoke.allow(credential.device, now_ms) {
                Some(Limit::Revoke)
            } else {
                None
            }
        };
        if self.refused(refused) {
            return Reply::Limited;
        }
        let what = Revoked::Handle(handle.id);
        match self.store.revoke(&what, credential.expires_s).await {
            Ok(()) => Reply::Revoked,
            Err(error) => {
                tracing::error!(%error, "could not store a revocation");
                Reply::Unavailable
            }
        }
    }
}

/// What the caller hears of APNs's answer. The reason is one of Apple's fixed
/// names, so it is safe to log.
fn reply(outcome: Outcome) -> Reply {
    match outcome {
        Outcome::Delivered => Reply::Delivered,
        Outcome::Gone => Reply::Gone,
        Outcome::RateLimited => Reply::Limited,
        Outcome::Rejected { reason, status } => {
            tracing::warn!(status, reason = reason.as_deref(), "APNs refused a push");
            // 403 is the relay's own key, and 5xx is Apple: neither is the
            // caller's fault, and a later push may work.
            if status == 403 || status >= 500 {
                Reply::Unavailable
            } else {
                Reply::Refused
            }
        }
    }
}

/// A JSON object within APNs's bound.
fn valid_payload(payload: &str) -> bool {
    payload.len() <= MAX_PAYLOAD
        && serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(payload).is_ok()
}

/// What can go in a header and APNs takes.
fn valid_collapse_id(id: Option<&str>) -> bool {
    id.is_none_or(|id| {
        (1..=MAX_COLLAPSE_ID).contains(&id.len()) && id.bytes().all(|byte| byte.is_ascii_graphic())
    })
}
