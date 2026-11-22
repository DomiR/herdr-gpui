//! The agents a person marked as favorites, which every phone and Sheprd
//! list first whatever their order. Each computer's own daemon keeps its own
//! agents' favorites, by pane, so every device that reaches it shows the
//! same stars. A change names one pane, so two devices starring different
//! agents at once both count, and the latest change to a pane wins.
//!
//! Herdr 1 numbers panes from one again when it restarts, so the favorites
//! remember the boot of the primary session they were made in and drop its
//! panes when another boot answers. herdr-core panes carry their core's id
//! ([`core_pane`]) and named sessions their name, so neither is mistaken for
//! a pane of the new boot.

use serde::{Deserialize, Serialize};
use std::fmt::Write as _;

/// The most favorites one computer keeps; past it, the oldest go.
pub const MAX_FAVORITES: usize = 256;
/// The longest pane id, in bytes.
pub const MAX_PANE_BYTES: usize = 256;

/// What prefixes a herdr-core pane's id.
const CORE_PREFIX: &str = "core:";
/// What prefixes the ids of a named Herdr 1 session's panes.
const SESSION_PREFIX: &str = "herdr:";

/// A computer's favorite agents, by pane, the oldest first.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct Favorites {
    /// The primary Herdr 1 session's boot its panes were starred in; none
    /// before one answered.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub boot: Option<String>,
    #[serde(default)]
    pub panes: Vec<String>,
}

/// One pane starred or unstarred.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Change {
    pub pane: String,
    pub favorite: bool,
}

impl Favorites {
    pub fn contains(&self, pane: &str) -> bool {
        self.panes.iter().any(|favorite| favorite == pane)
    }

    /// Within bounds: no blank, oversized, or repeated pane, and at most
    /// [`MAX_FAVORITES`], the newest kept.
    #[must_use]
    pub fn bounded(mut self) -> Self {
        let mut kept: Vec<String> = Vec::with_capacity(self.panes.len());
        for pane in self.panes.into_iter().rev() {
            if kept.len() == MAX_FAVORITES {
                break;
            }
            if valid_pane(&pane) && !kept.contains(&pane) {
                kept.push(pane);
            }
        }
        kept.reverse();
        self.panes = kept;
        self
    }

    /// With `change` made: a starred pane goes last, as the newest.
    /// `None` when the pane is not a valid id.
    #[must_use]
    pub fn changed(mut self, change: Change) -> Option<Self> {
        if !valid_pane(&change.pane) {
            return None;
        }
        self.panes.retain(|pane| *pane != change.pane);
        if change.favorite {
            self.panes.push(change.pane);
        }
        Some(self.bounded())
    }

    /// Follows the primary session's `boot`: when it differs from the one
    /// these were starred in, that session's panes are other panes now.
    /// Returns whether anything changed.
    pub fn follow_boot(&mut self, boot: &str) -> bool {
        if boot.is_empty() || self.boot.as_deref() == Some(boot) {
            return false;
        }
        if self.boot.is_some() {
            self.panes.retain(|pane| !primary_pane(pane));
        }
        self.boot = Some(boot.to_owned());
        true
    }
}

fn valid_pane(pane: &str) -> bool {
    !pane.trim().is_empty() && pane.len() <= MAX_PANE_BYTES && !pane.chars().any(char::is_control)
}

/// Whether `pane` belongs to the primary Herdr 1 session.
fn primary_pane(pane: &str) -> bool {
    !pane.starts_with(CORE_PREFIX) && !pane.starts_with(SESSION_PREFIX)
}

/// The id phones and favorites know the herdr-core terminal at `address`
/// by, on the core whose id is `core`. Addresses survive a core restart but
/// start over when its state is wiped, which also gives the core a new id:
/// naming the core keeps a favorite or an open phone view from landing on
/// an unrelated new terminal. Hex keeps any address in one URL segment.
pub fn core_pane(core: &str, address: &str) -> String {
    let tag = core.bytes().fold(0x811c_9dc5_u32, |hash, byte| {
        (hash ^ u32::from(byte)).wrapping_mul(0x0100_0193)
    });
    let mut id = format!("{CORE_PREFIX}{tag:08x}-");
    for byte in address.bytes() {
        let _ = write!(id, "{byte:02x}");
    }
    id
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests;
