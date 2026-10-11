//! The worker's Linear source: its API key and workspace, and its listing,
//! which is kept in memory and never written to the shared database.

use super::{Notice, SyncState, Worker, push};
use crate::orchestrator::{Error, Provider, Result, linear};
use chrono::Utc;
use secrecy::SecretString;
use std::sync::{Arc, atomic::Ordering};

/// The Linear API key as last read. It is read when the view opens, on
/// Refresh, and after a save, never on the periodic sync: a locked macOS
/// Keychain would ask every minute in every open view.
pub(super) enum LinearKey {
    Missing,
    Found(SecretString),
    Unreadable(Arc<Error>),
}

impl Worker {
    /// The workspace the Linear source is keyed by, `None` when the view
    /// leaves Linear out. It is empty until a sync learns it.
    pub(super) fn linear_source(&self) -> Option<&str> {
        self.request
            .linear
            .map(|_| self.linear_workspace.as_deref().unwrap_or(""))
    }

    pub(super) fn read_linear_key(&mut self) {
        let Some(store) = self.request.linear else {
            return;
        };
        self.set_linear_key(match linear::read_key(store) {
            Ok(Some(key)) => LinearKey::Found(key),
            Ok(None) => LinearKey::Missing,
            Err(error) => LinearKey::Unreadable(Arc::new(error)),
        });
    }

    /// Takes `key` as the Linear key: Linear syncs at the next chance even
    /// after a failure, and without a key its listing goes.
    pub(super) fn set_linear_key(&mut self, key: LinearKey) {
        self.retry
            .retain(|(source, _)| source.provider != Provider::Linear);
        self.linear_key = key;
        if matches!(self.linear_key, LinearKey::Missing) {
            let before = self.uncached.len();
            self.uncached
                .retain(|source, _| source.provider != Provider::Linear);
            if self.uncached.len() != before {
                if let Err(error) = self.reload() {
                    self.snapshot.error = Some(Arc::new(error));
                }
                self.publish();
            }
        }
    }

    /// Saves `key`, or with `None` removes it, then reads the key again.
    /// Saving needs no writable database: the key is not stored there.
    pub(super) fn save_linear_key(&mut self, key: Option<&SecretString>) {
        let Some(store) = self.request.linear else {
            return;
        };
        let outcome = linear::save_key(store, key)
            .map(|()| match key {
                Some(_) => "Linear connected",
                None => "Linear key removed",
            })
            .map_err(Arc::new);
        push(&self.notices, Notice { outcome });
        self.read_linear_key();
    }

    pub(super) fn sync_linear(&mut self) -> Result<SyncState> {
        let key = match &self.linear_key {
            LinearKey::Missing => return Ok(SyncState::SignIn),
            LinearKey::Unreadable(error) => return Ok(SyncState::Failed(error.clone())),
            LinearKey::Found(key) => key,
        };
        let (fetch, cancelled) = (self.linear_fetch, &self.cancelled);
        let listing = linear::sync(
            |variables| fetch(key, variables),
            || cancelled.load(Ordering::Relaxed),
        )?;
        let count = listing.items.len();
        let old = linear::source(self.linear_workspace.as_deref().unwrap_or(""));
        let new = linear::source(&listing.workspace);
        if let Some(status) = self
            .snapshot
            .sources
            .iter_mut()
            .find(|status| status.key == old)
        {
            status.key = new.clone();
        }
        self.linear_workspace = Some(listing.workspace);
        self.uncached
            .retain(|source, _| source.provider != Provider::Linear);
        self.uncached.insert(new, listing.items);
        Ok(if listing.complete {
            SyncState::Synced {
                at: Utc::now(),
                count,
            }
        } else {
            SyncState::Capped { count }
        })
    }
}
