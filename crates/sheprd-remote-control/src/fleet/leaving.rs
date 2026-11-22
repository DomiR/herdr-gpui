//! Leaving a fleet for good. A computer that left, or was removed, keeps
//! the log until its removal has been passed on: until a live computer
//! member said it holds the removal (`Inner::held`, which the removal
//! flood fills), or none is left to tell. Then the fleet is forgotten
//! here, so this computer may found or join another. A computer that
//! joined as a shared host forgets the fleet once the fleet removed that
//! host: it has nothing to pass on, and lets nobody in any more.

use super::{Fleet, Inner, Result, store};
use crate::control::fleet::{Body, Kind, Log, MemberId, OpId};
use std::{collections::BTreeSet, sync::atomic::Ordering};

/// Whether `me` is out of `inner`'s fleet for good: removed, and its
/// removal reached another live computer member, or none is left.
pub(crate) fn released(inner: &Inner, me: &MemberId) -> bool {
    let Some(log) = &inner.log else {
        return false;
    };
    if log.is_member(me) || !log.state().tombstones.contains(me) {
        return false;
    }
    let removals = removals_of(log, me);
    let mut others = log
        .state()
        .members
        .iter()
        .filter(|(id, member)| *id != me && member.bundle.kind == Kind::Computer)
        .peekable();
    others.peek().is_none()
        || others.any(|(id, _)| {
            inner
                .held
                .get(&super::Holder::Member(*id))
                .is_some_and(|held| !held.is_disjoint(&removals))
        })
}

/// The removals in `log` that remove `me`.
fn removals_of(log: &Log, me: &MemberId) -> BTreeSet<OpId> {
    log.ops()
        .filter(|op| matches!(op.body(), Body::Remove(remove) if remove.targets().any(|target| target == me)))
        .map(|op| op.id())
        .collect()
}

impl Fleet {
    /// Forgets the fleet once this computer is out of it for good
    /// ([`released`]): its log, what peers hold, and suspensions. The keys
    /// members proved here are kept, and saved, until `grants::apply`
    /// revokes the pairings made with them: with no log, it treats every
    /// one as removed. Says whether it did. Blocking.
    pub(crate) fn release_if_left(&self, inner: &mut Inner) -> Result<bool> {
        let host_removed = self.hosted.load(Ordering::SeqCst)
            && inner.log.as_ref().is_some_and(|log| {
                !log.is_member(&self.me)
                    && self
                        .host
                        .is_none_or(|host| !log.state().hosts.contains_key(&host))
            });
        if !host_removed && !released(inner, &self.me) {
            return Ok(false);
        }
        let proven = std::mem::take(&mut inner.proven);
        *inner = Inner::default();
        inner.proven = proven;
        store::forget(&self.state_dir)?;
        self.save(inner)?;
        self.hosted.store(false, Ordering::SeqCst);
        self.bump();
        Ok(true)
    }

    /// Whether this computer is in a fleet, after forgetting one it is out
    /// of for good. Blocking.
    pub(crate) fn in_fleet(&self) -> Result<bool> {
        let mut inner = self.inner();
        self.release_if_left(&mut inner)?;
        Ok(inner.log.is_some())
    }
}
