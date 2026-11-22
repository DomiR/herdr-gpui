//! Plain hosts' access files, as Sheprd asks for them over the control
//! socket: computed from the log here, so the log never leaves the daemon
//! (`control::fleet::access`).
//!
//! An agent reports the removals it suspended members for. One the log
//! has not heard of may be on its way, or one a member made up to keep
//! another suspended. It is settled once there is evidence it is not
//! coming (`access::compute`), or once it was reported unknown in at least
//! [`GRACE_SYNCS`] syncs over at least [`GRACE_MS`], and at the latest
//! after the [`UNCONFIRMED_TTL_MS`] a daemon keeps its own unconfirmed
//! suspensions.

use super::{Error, Fleet, Result, UNCONFIRMED_TTL_MS};
use crate::control::fleet::{
    HostId, OpId,
    access::{self, Access, Asked},
};
use serde::{Deserialize, Serialize};

/// Syncs a removal stays unknown before it is settled.
pub(crate) const GRACE_SYNCS: u32 = 3;
/// How long a removal stays unknown before it is settled.
pub(crate) const GRACE_MS: u64 = 60 * 60 * 1000;
/// Unknown removals remembered at once.
const MAX_UNKNOWN: usize = 1024;

/// A removal the log has not heard of: when an agent first reported it,
/// and in how many syncs.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(crate) struct Unknown {
    pub(crate) first_ms: u64,
    pub(crate) syncs: u32,
}

impl Unknown {
    fn graced(self, now_ms: u64) -> bool {
        let age = now_ms.saturating_sub(self.first_ms);
        (self.syncs >= GRACE_SYNCS && age >= GRACE_MS) || age >= UNCONFIRMED_TTL_MS
    }
}

impl Fleet {
    /// The access file for plain host `host`, settling what its agent
    /// reported suspending for, at `now_ms`; counts this sync for each
    /// removal the log has not heard of. Blocking: it writes the fleet's
    /// files.
    pub(crate) fn agent_access(
        &self,
        host: &HostId,
        asked: Vec<Asked>,
        now_ms: u64,
    ) -> Result<Access> {
        let mut inner = self.inner();
        let inner = &mut *inner;
        let log = inner.log.as_ref().ok_or(Error::NoFleet)?;
        let mut ids: Vec<OpId> = asked.iter().map(|asked| asked.removal).collect();
        ids.sort_unstable();
        ids.dedup();
        for id in ids.iter().copied().filter(|id| log.get(id).is_none()) {
            let seen = inner.unknown.entry(id).or_insert(Unknown {
                first_ms: now_ms,
                syncs: 0,
            });
            seen.syncs = seen.syncs.saturating_add(1);
        }
        // Heard of now: nothing left to count. Long past every grace and
        // reported no more: forgotten. One still reported is kept however
        // old, so its wait is not started over.
        inner.unknown.retain(|id, seen| {
            log.get(id).is_none()
                && (ids.contains(id)
                    || now_ms.saturating_sub(seen.first_ms) <= 2 * UNCONFIRMED_TTL_MS)
        });
        while inner.unknown.len() > MAX_UNKNOWN {
            let Some(oldest) = inner
                .unknown
                .iter()
                .min_by_key(|(_, seen)| seen.first_ms)
                .map(|(id, _)| *id)
            else {
                break;
            };
            inner.unknown.remove(&oldest);
        }
        let unknown = &inner.unknown;
        let graced = |id: &OpId| unknown.get(id).is_some_and(|seen| seen.graced(now_ms));
        let file = access::compute(log, host, asked, graced)?;
        self.save(inner)?;
        Ok(file)
    }
}
