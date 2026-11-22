//! This computer as the fleet knows it: its member bundle, and the record
//! of where it listens, signed by its certificate authority so no other
//! member can speak for it.

use super::{Error, Fleet, Result};
use crate::control::fleet::{Bundle, Ca, HostId, HostIntro, HostKind, Kind, SelfRecord, Verified};
use crate::web::State;
use sha2::{Digest, Sha256};

/// This computer's bundle, with its host record as of `now_ms`. Blocking:
/// it reads the desktop keys and the SSH host keys.
pub(crate) fn bundle(state: &State, fleet: &Fleet, now_ms: u64) -> Result<Bundle> {
    let keys = crate::desktop::keys::Keys::load_or_create(fleet.state_dir())?;
    let ssh = crate::desktop::keys::ssh_public_key(fleet.state_dir())?;
    let host = intro(state, now_ms)?;
    Ok(Bundle {
        key: point(fleet.key().verifying_key())?,
        kind: Kind::Computer,
        name: name(&state.computer),
        dial: point(keys.dial.verifying_key())?,
        tunnel: keys.tunnel.identity().x25519(),
        ssh: Some(ssh),
        host: Some(host),
    })
}

/// This computer as a host, its record signed by its certificate
/// authority. Blocking.
pub(crate) fn intro(state: &State, now_ms: u64) -> Result<HostIntro> {
    let (record, signature) = record(state, now_ms)?;
    let authority = state.authority.as_ref().ok_or(Error::NoAuthority)?;
    let login = crate::ssh_access::Login::here(&state.auth().ssh_access()?.host_key_dir)?;
    Ok(HostIntro {
        kind: HostKind::Member,
        name: name(&state.computer),
        user: login.user,
        ca: Some(Ca {
            certificate: authority.ca_der().to_vec(),
            key: authority.public_key().ok_or(Error::NoAuthority)?,
        }),
        record,
        record_signature: Some(signature),
        verified: Verified::KnownHosts,
    })
}

/// Where this computer listens now, with `rev` set to `now_ms` so a later
/// record replaces it, and its certificate authority's signature.
/// Blocking.
pub(crate) fn record(state: &State, now_ms: u64) -> Result<(SelfRecord, [u8; 64])> {
    let authority = state.authority.as_ref().ok_or(Error::NoAuthority)?;
    let info = state
        .relay
        .as_ref()
        .and_then(|relay| relay.offer())
        .filter(|info| !info.relays.is_empty())
        .ok_or(Error::NoRelays)?;
    let login = crate::ssh_access::Login::here(&state.auth().ssh_access()?.host_key_dir)?;
    let record = SelfRecord {
        host: HostId(Sha256::digest(authority.ca_der()).into()),
        rev: now_ms,
        route: info.route,
        relays: info.relays.into_iter().take(16).collect(),
        identity: info.identity,
        direct: Vec::new(),
        ssh_host_keys: login.host_keys.into_iter().take(4).collect(),
        signed_ms: now_ms,
    };
    let message = record
        .message()
        .ok_or(Error::Log(crate::control::fleet::Error::Invalid))?;
    let signature = authority.sign(&message).ok_or(Error::NoAuthority)?;
    Ok((record, signature))
}

/// The record this computer would publish now differs from what the fleet
/// holds for it: relays, route, tunnel identity, or SSH host keys moved.
pub(crate) fn moved(held: &SelfRecord, now: &SelfRecord) -> bool {
    held.route != now.route
        || held.relays != now.relays
        || held.identity != now.identity
        || held.ssh_host_keys != now.ssh_host_keys
}

fn point(key: &p256::ecdsa::VerifyingKey) -> Result<[u8; 33]> {
    key.to_sec1_point(true)
        .as_bytes()
        .try_into()
        .map_err(|_| Error::Log(crate::control::fleet::Error::Invalid))
}

/// A member name: the computer's, without control characters, cut to 64
/// bytes on a character boundary.
fn name(computer: &str) -> String {
    let mut name: String = computer.chars().filter(|char| !char.is_control()).collect();
    while name.len() > 64 {
        name.pop();
    }
    if name.trim().is_empty() {
        "computer".to_owned()
    } else {
        name
    }
}
