//! The worker's Linear source: its API key and workspace, its listing,
//! which is kept in memory and never written to the shared database, and
//! moving an issue to another workflow state.

use super::{Command, Notice, Service, SyncState, Worker, push};
use crate::{Error, Provider, Result, linear};
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

impl Service {
    /// Asks for the Linear API key to be saved, or with `None` removed.
    /// `false` means the worker is behind and nothing was queued.
    pub fn save_linear_key(&self, key: Option<SecretString>) -> bool {
        self.commands.try_send(Command::SaveLinearKey(key)).is_ok()
    }

    /// Asks for Linear to sync now, outside its interval, as when its
    /// section shows again; a full queue drops it.
    pub fn sync_linear(&self) {
        let _ = self.commands.try_send(Command::SyncLinear);
    }

    /// Asks for a Linear issue to move. `false` means the worker is behind
    /// and nothing was queued.
    pub fn move_linear_issue(&self, change: linear::Move) -> bool {
        self.commands
            .try_send(Command::MoveLinearIssue(change))
            .is_ok()
    }
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
        self.retry.forget(Provider::Linear);
        self.linear_states = None;
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
    /// Saving needs no writable database: the key is not stored there. The
    /// new issue dialog's choices, which another key may not see, are read
    /// again when it next opens.
    pub(super) fn save_linear_key(&mut self, key: Option<&SecretString>) {
        let Some(store) = self.request.linear else {
            return;
        };
        self.snapshot.linear_create.options = None;
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
        if self.linear_states.is_none() {
            // Without the states the listing still shows; its cards cannot
            // move, and the next sync reads them again.
            match linear::states(|query, variables| fetch(key, query, variables)) {
                Ok(states) => self.linear_states = Some(states),
                Err(Error::Cancelled) => return Err(Error::Cancelled),
                Err(error) => {
                    tracing::warn!(category = "linear", %error, "Cannot read Linear's workflow states");
                }
            }
        }
        let none = linear::States::new();
        let listing = linear::sync(
            |query, variables| fetch(key, query, variables),
            self.linear_states.as_ref().unwrap_or(&none),
            || cancelled.load(Ordering::Relaxed),
        )?;
        if listing.missing_states {
            self.linear_states = None;
        }
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

    /// Moves an issue as `change` asks. Moved, the listing shows the state
    /// and place Linear answered with at once, before the sync that follows, so a
    /// sync Linear refuses never puts the card back; refused, the notice
    /// says why. Either way the snapshot then echoes `change`'s sequence,
    /// and the view stops showing the move ahead of Linear. Whether it
    /// moved.
    pub(super) fn move_linear_issue(&mut self, change: &linear::Move) -> bool {
        let fetch = self.linear_fetch;
        let result = match &self.linear_key {
            LinearKey::Found(key) => {
                linear::move_issue(|query, variables| fetch(key, query, variables), change)
                    .map_err(Arc::new)
            }
            LinearKey::Missing => Err(Arc::new(Error::LinearKeyMissing)),
            LinearKey::Unreadable(error) => Err(error.clone()),
        };
        let moved = match result {
            Ok(placed) => {
                for item in self
                    .uncached
                    .iter_mut()
                    .filter(|(source, _)| source.provider == Provider::Linear)
                    .flat_map(|(_, items)| items.iter_mut())
                    .filter(|item| item.key.native_id == change.identifier)
                {
                    if let Some(tracker) = item.tracker.as_mut() {
                        item.state = placed.status.name.to_lowercase();
                        tracker.status = Some(placed.status.clone());
                        tracker.sort_order = placed.sort_order.or(tracker.sort_order);
                    }
                }
                if let Err(error) = self.reload() {
                    self.snapshot.error = Some(Arc::new(error));
                }
                true
            }
            Err(source) => {
                // Linear refused the state, which may be gone: the next
                // sync reads the states again.
                if change.state.is_some()
                    && matches!(
                        *source,
                        Error::LinearStatus { status, .. } if status < 500
                    )
                {
                    self.linear_states = None;
                }
                push(
                    &self.notices,
                    Notice {
                        outcome: Err(Arc::new(Error::LinearMove {
                            issue: change.identifier.clone(),
                            source,
                        })),
                    },
                );
                false
            }
        };
        self.snapshot.linear_moved = change.sequence;
        self.publish();
        moved
    }
}
