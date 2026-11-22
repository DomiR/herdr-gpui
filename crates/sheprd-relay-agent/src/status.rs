//! What `status --json` reports, for Sheprd to read over SSH when it
//! enrolls this machine: the route and identity to pin, how many desktops
//! may dial, the fleet it belongs to and the digest of its access file,
//! and, while the agent runs, each relay's state.
//!
//! The running agent writes `status.json` in its state directory whenever
//! a relay's state changes and every [`HEARTBEAT`], and removes it when it
//! stops. A file older than [`STALE`] is from an agent that died.

use crate::{Error, Result, files, fleet};
#[cfg(feature = "runtime")]
use crate::{authorized::Authorized, keys::Keys};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    path::Path,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

pub const FILE: &str = "status.json";
/// How often the running agent rewrites its status.
pub const HEARTBEAT: Duration = Duration::from_secs(30);
/// A status older than this is from an agent that is no longer running.
pub const STALE: Duration = Duration::from_secs(90);
const MAX_BYTES: u64 = 64 * 1024;
/// Where OpenSSH keeps the host keys.
pub const HOST_KEY_DIR: &str = "/etc/ssh";
/// Host key files, the ones OpenSSH makes by default, strongest first.
const HOST_KEYS: [&str; 3] = [
    "ssh_host_ed25519_key.pub",
    "ssh_host_ecdsa_key.pub",
    "ssh_host_rsa_key.pub",
];
/// The longest host key file read.
const MAX_HOST_KEY: u64 = 16 * 1024;

/// The most of `status --json` Sheprd reads over SSH: twice what the
/// largest report the agent's bounds allow takes, printed.
pub const MAX_REPORT_BYTES: usize = 256 * 1024;

/// The longest relay failure reported, in bytes.
pub const MAX_RELAY_ERROR: usize = 512;

/// Cuts `error` to [`MAX_RELAY_ERROR`] bytes, on a character boundary,
/// with control characters, which JSON would spell out six bytes each,
/// as spaces.
pub fn bound_error(error: &mut String) {
    if error.chars().any(char::is_control) {
        *error = error
            .chars()
            .map(|c| if c.is_control() { ' ' } else { c })
            .collect();
    }
    if error.len() <= MAX_RELAY_ERROR {
        return;
    }
    let mut end = MAX_RELAY_ERROR;
    while !error.is_char_boundary(end) {
        end -= 1;
    }
    error.truncate(end);
}

/// One relay's state.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case", tag = "state")]
pub enum RelayState {
    Connecting,
    Listening,
    Failed { error: String },
}

/// What the running agent writes.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct Live {
    /// Each relay, by endpoint.
    pub relays: BTreeMap<String, RelayState>,
    /// When it was written, in seconds since the Unix epoch.
    pub updated: u64,
}

impl Live {
    /// Writes this status to `state_dir`, stamped now. Blocking.
    pub fn save(&mut self, state_dir: &Path, now: SystemTime) -> Result<()> {
        self.updated = seconds(now);
        let path = state_dir.join(FILE);
        let json = serde_json::to_vec_pretty(self).map_err(|source| Error::StatusFormat {
            path: path.clone(),
            source,
        })?;
        files::write_private(&path, &json)
    }

    /// The running agent's status in `state_dir`, unless none is running:
    /// no file, or one older than [`STALE`] at `now`. Blocking.
    pub fn load(state_dir: &Path, now: SystemTime) -> Result<Option<Self>> {
        let path = state_dir.join(FILE);
        let Some(bytes) = files::read_bounded(&path, MAX_BYTES)? else {
            return Ok(None);
        };
        let live: Self = serde_json::from_slice(&bytes)
            .map_err(|source| Error::StatusFormat { path, source })?;
        let age = seconds(now).saturating_sub(live.updated);
        Ok((age <= STALE.as_secs()).then_some(live))
    }
}

fn seconds(time: SystemTime) -> u64 {
    time.duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_secs())
}

/// What `status` prints.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Report {
    /// How many desktops enrolled one by one may dial.
    pub authorized: usize,
    /// The fleet this account belongs to, once Sheprd gave it an access
    /// file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fleet: Option<fleet::Report>,
    /// The tunnel identity desktops pin, base64url.
    pub identity: String,
    /// Each relay's state while the agent runs; empty when it does not.
    pub relays: BTreeMap<String, RelayState>,
    /// The route this agent listens on, base64url.
    pub route: String,
    /// Whether an agent is running for this state directory.
    pub running: bool,
    /// The SSH server's host keys, `TYPE BASE64`, strongest first: what
    /// Sheprd checks against its own strictly checked session before it
    /// shares this machine, and the first of which names it in the fleet.
    #[serde(default)]
    pub ssh_host_keys: Vec<String>,
    /// The agent's version.
    pub version: String,
}

impl Report {
    /// The report for `state_dir` at `now`, making the keys the first time
    /// so a machine being enrolled has its route before the agent starts.
    /// Blocking.
    #[cfg(feature = "runtime")]
    pub fn read(state_dir: &Path, now: SystemTime) -> Result<Self> {
        let keys = Keys::load_or_create(state_dir)?;
        let authorized = Authorized::load(state_dir)?;
        let live = Live::load(state_dir, now)?;
        let fleet = fleet::Held::load(state_dir)?.report(&authorized)?;
        Ok(Self {
            authorized: authorized.entries().len(),
            fleet,
            identity: keys.identity(),
            relays: live
                .as_ref()
                .map(|live| live.relays.clone())
                .unwrap_or_default(),
            route: keys.route().to_string(),
            running: live.is_some(),
            ssh_host_keys: host_keys(Path::new(HOST_KEY_DIR)),
            version: env!("CARGO_PKG_VERSION").to_owned(),
        })
    }

    /// Reads `status --json` as the agent printed it, at most
    /// [`MAX_REPORT_BYTES`].
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        if bytes.len() > MAX_REPORT_BYTES {
            return Err(Error::TooLarge {
                limit: MAX_REPORT_BYTES as u64,
                path: FILE.into(),
            });
        }
        serde_json::from_slice(bytes).map_err(|source| Error::StatusFormat {
            path: FILE.into(),
            source,
        })
    }

    /// The report as a person reads it.
    #[must_use]
    pub fn text(&self) -> String {
        let mut text = format!(
            "route: {}\nrunning: {}\nauthorized desktops: {}\n",
            self.route,
            if self.running { "yes" } else { "no" },
            self.authorized
        );
        if let Some(fleet) = &self.fleet {
            text.push_str(&format!(
                "fleet {}: {} members may reach SSH, {} may not, {} removed, {} suspended\n",
                fleet.fleet.short(),
                fleet.allowed,
                fleet.denied,
                fleet.removed,
                fleet.suspended.len()
            ));
        }
        for (relay, state) in &self.relays {
            let state = match state {
                RelayState::Connecting => "connecting".to_owned(),
                RelayState::Listening => "listening".to_owned(),
                RelayState::Failed { error } => format!("failed: {error}"),
            };
            text.push_str(&format!("relay {relay}: {state}\n"));
        }
        text
    }
}

/// The SSH host keys in `dir`, `TYPE BASE64`, strongest first; files that
/// are missing or hold no key are passed over. Blocking.
pub fn host_keys(dir: &Path) -> Vec<String> {
    HOST_KEYS
        .iter()
        .filter_map(|name| {
            files::read_bounded(&dir.join(name), MAX_HOST_KEY)
                .ok()
                .flatten()
        })
        .filter_map(|bytes| host_key(&String::from_utf8_lossy(&bytes)))
        .collect()
}

/// `TYPE BASE64` from a host key file's first line.
fn host_key(text: &str) -> Option<String> {
    let mut fields = text.lines().next()?.split_ascii_whitespace();
    let (kind, blob) = (fields.next()?, fields.next()?);
    let kind_valid = (kind.starts_with("ssh-") || kind.starts_with("ecdsa-"))
        && kind
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"-@.".contains(&byte));
    let blob_valid = !blob.is_empty()
        && blob
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"+/=".contains(&byte));
    (kind_valid && blob_valid).then(|| format!("{kind} {blob}"))
}

#[cfg(test)]
mod tests;
