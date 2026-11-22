//! Phones asking to join the fleet through this computer
//! (`POST /api/v2/fleet/join`), waiting for a person to admit them in
//! Sheprd (`Ask::Joins`, then `Ask::Admit`).
//!
//! Nothing here ever admits anyone: a join is only kept, in memory, for
//! [`TTL_MS`], at most [`MAX`] at once and one per paired device, so
//! Sheprd can show it. The joiner's signature is checked on arrival, so a
//! join names a key its owner holds, and a removed key is refused at once.

mod error;

pub use error::{Error, Result};

use crate::control::{
    DeviceId,
    fleet::{Bundle, Kind, Log, MemberId, sync},
};
use p256::ecdsa::{Signature, VerifyingKey, signature::Verifier};
use std::collections::{BTreeMap, HashSet};

/// Joins waiting at once.
pub(crate) const MAX: usize = 8;
/// How long a join waits for a person's approval.
pub(crate) const TTL_MS: u64 = 10 * 60 * 1000;

/// Where a joiner stands.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Standing {
    /// Waiting for a person to admit it in Sheprd.
    Pending,
    /// Already a member.
    Member,
}

#[derive(Debug)]
struct Waiting {
    bundle: Bundle,
    signature: [u8; 64],
    device: DeviceId,
    /// The device's name here, for Sheprd's dialog.
    device_name: String,
    received_ms: u64,
}

/// The joins waiting, memory only.
#[derive(Debug, Default)]
pub(crate) struct Joins {
    waiting: BTreeMap<MemberId, Waiting>,
}

impl Joins {
    /// Keeps a join from `device` for `log`'s fleet, replacing that
    /// device's earlier one. A member already in the fleet is not kept.
    pub(crate) fn ask(
        &mut self,
        log: &Log,
        bundle: Bundle,
        signature: [u8; 64],
        device: (DeviceId, String),
        now_ms: u64,
    ) -> Result<Standing> {
        self.expire(now_ms);
        let member = bundle.id();
        if bundle.kind != Kind::Phone {
            return Err(Error::NotPhone);
        }
        if log.state().tombstones.contains(&member) {
            return Err(Error::Removed);
        }
        let message = bundle
            .join_message(&log.fleet())
            .ok_or(crate::control::fleet::Error::Invalid)?;
        let key = VerifyingKey::from_sec1_bytes(&bundle.key).map_err(|_| Error::Signature)?;
        let parsed = Signature::from_slice(&signature).map_err(|_| Error::Signature)?;
        key.verify(&message, &parsed)
            .map_err(|_| Error::Signature)?;
        if log.is_member(&member) {
            return Ok(Standing::Member);
        }
        let (device, device_name) = device;
        self.waiting
            .retain(|id, waiting| *id != member && waiting.device != device);
        if self.waiting.len() >= MAX {
            return Err(Error::Full);
        }
        self.waiting.insert(
            member,
            Waiting {
                bundle,
                signature,
                device,
                device_name,
                received_ms: now_ms,
            },
        );
        Ok(Standing::Pending)
    }

    /// Whether `member` is waiting.
    pub(crate) fn pending(&mut self, member: &MemberId, now_ms: u64) -> bool {
        self.expire(now_ms);
        self.waiting.contains_key(member)
    }

    /// The joins waiting, oldest first, as Sheprd lists them: only those
    /// from a device `paired` still holds. One revoked after asking is not
    /// listed, so nothing admits it, by hand or by its device.
    pub(crate) fn list(&mut self, now_ms: u64, paired: &HashSet<DeviceId>) -> Vec<sync::Join> {
        self.expire(now_ms);
        let mut joins: Vec<sync::Join> = self
            .waiting
            .iter()
            .filter(|(_, waiting)| paired.contains(&waiting.device))
            .filter_map(|(member, waiting)| {
                Some(sync::Join {
                    member: member.to_string(),
                    name: waiting.bundle.name.clone(),
                    kind: waiting.bundle.kind,
                    bundle: sync::text(&waiting.bundle.to_bytes().ok()?),
                    signature: sync::text(&waiting.signature),
                    device: waiting.device_name.clone(),
                    device_id: waiting.device.clone(),
                    received_ms: waiting.received_ms,
                })
            })
            .collect();
        joins.sort_by_key(|join| join.received_ms);
        joins
    }

    /// Drops `member`'s join: a person admitted it.
    pub(crate) fn admitted(&mut self, member: &MemberId) {
        self.waiting.remove(member);
    }

    /// Refuses admitting `member` when its join came from a device
    /// `paired` no longer holds: revoking the device ends its request too.
    /// A member that asked through no join here (over SSH, say) passes.
    pub(crate) fn admissible(&self, member: &MemberId, paired: &HashSet<DeviceId>) -> Result<()> {
        match self.waiting.get(member) {
            Some(waiting) if !paired.contains(&waiting.device) => Err(Error::Unpaired),
            _ => Ok(()),
        }
    }

    /// Drops the join `device` asked for, as it is revoked.
    pub(crate) fn forget_device(&mut self, device: &DeviceId) {
        self.waiting.retain(|_, waiting| waiting.device != *device);
    }

    fn expire(&mut self, now_ms: u64) {
        self.waiting
            .retain(|_, waiting| now_ms.saturating_sub(waiting.received_ms) < TTL_MS);
    }
}
