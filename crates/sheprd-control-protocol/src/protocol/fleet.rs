//! The fleet (`docs/relay-plan.md`, Fleet): what the daemon keeps of it,
//! and the window's Settings > Fleet page, where a person confirms every
//! change. Nothing is signed from the command line.
use serde::{Deserialize, Serialize};
use sheprd_remote_control::control::fleet::View;

/// Asking the daemon takes up to its own answer timeout.
const DAEMON_WAIT_SECONDS: u64 = 12;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum FleetOp {
    /// The fleet as this computer's daemon keeps it: members and hosts.
    Status,
    /// Shows Settings > Fleet, where computers are invited and hosts shared.
    Open,
    /// Shows Settings > Fleet asking why to remove a member, named by its
    /// ID or name; the user confirms there.
    Remove { member: String },
    /// Shows Settings > Fleet asking to remove a host everywhere, by its ID
    /// or name; the user confirms there.
    RemoveHost { host: String },
    /// Shows Settings > Fleet asking to remove every way a fleet reaches a
    /// machine this Sheprd connects to, by its device ID or name; the user
    /// confirms there.
    Scrub { machine: String },
}

impl FleetOp {
    pub fn wait_seconds(&self) -> u64 {
        match self {
            // A removal reads the daemon's view before naming its target.
            Self::Status | Self::Remove { .. } | Self::RemoveHost { .. } => DAEMON_WAIT_SECONDS,
            Self::Open | Self::Scrub { .. } => 0,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Reply {
    Status {
        /// What the daemon keeps, when this computer is in a fleet.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        view: Option<Box<View>>,
        /// Why the daemon did not answer.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        error: Option<String>,
    },
}

#[cfg(test)]
mod tests;
