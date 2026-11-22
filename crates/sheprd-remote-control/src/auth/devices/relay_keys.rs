//! One device per relay key. The relays let a device through by its dial
//! key, and the tunnel names the device by its tunnel key, so two records
//! holding one key would let either's sessions pass as the other's.
//!
//! Keys are public: anyone may name another device's, and setting them
//! (pairing, `POST /api/relay`, a carried `host_relay` request, a fleet
//! link) proves nothing about holding their private halves. So no record
//! takes keys another holds, a device the fleet admitted included: taking
//! them would cut off whoever held them first. Every clash is refused.

use super::{Devices, RelayKeys};
use crate::auth::{Error, Result};
use crate::control::DeviceId;

/// What the ids of devices the fleet admits start with (`fleet::links`).
pub(crate) const FLEET_ID: &str = "fleet-";
/// What their `via` starts with.
pub(crate) const FLEET_VIA: &str = "fleet ";

/// Whether `keys` and `other` share a dial or a tunnel key.
fn clash(keys: &RelayKeys, other: &RelayKeys) -> bool {
    let (dial, tunnel) = (keys.dial_key(), keys.tunnel_key());
    (dial.is_some() && other.dial_key() == dial)
        || (tunnel.is_some() && other.tunnel_key() == tunnel)
}

impl Devices {
    /// Whether a record other than `id` holds any of `keys`.
    pub(crate) fn relay_taken(&self, id: &DeviceId, keys: &RelayKeys) -> bool {
        self.records().iter().any(|record| {
            record.id != *id
                && record
                    .relay
                    .as_ref()
                    .is_some_and(|other| clash(keys, other))
        })
    }

    /// Refuses `keys` for `id` when another record holds any of them.
    pub(super) fn ensure_relay_free(&self, id: &DeviceId, keys: Option<&RelayKeys>) -> Result<()> {
        match keys {
            Some(keys) if self.relay_taken(id, keys) => Err(Error::RelayKeysTaken),
            _ => Ok(()),
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests;
