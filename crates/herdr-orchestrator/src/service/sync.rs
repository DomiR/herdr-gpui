//! One sync of a repository's sources: finding the repository again when
//! its host was unreached, its branches, then each source in turn, Linear
//! on an interval of its own.

use super::{
    Access, Error, Provider, Result, SourceKey, SyncState, Worker, beads, github, gitlab, naming,
    sources::sources, ssh_slots,
};
use chrono::Utc;
use herdr_github::Tokens;
use herdr_remote::teleport::Host;
use std::{
    sync::{Arc, atomic::Ordering},
    time::Instant,
};

/// When a sync reads Linear.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Linear {
    /// Once its interval has passed since it last synced.
    Due,
    /// Now, as the user asked.
    Now,
}

impl Worker {
    /// Syncs every source this build reads, Linear as `linear` says.
    pub(super) fn sync(&mut self, token: Option<&Tokens>, linear: Linear) {
        if self.unreached {
            let found = Host::new(&self.request.target)
                .map_err(|_| Error::UnsupportedHost)
                .and_then(|host| self.find(&host));
            if let Err(error) = found
                && !matches!(error, Error::Cancelled)
            {
                self.snapshot.error = Some(Arc::new(error));
                self.publish();
            }
            // Still unreached: the sources' scripts would not reach it either.
            if self.unreached {
                return;
            }
        }
        self.recognize(gitlab::cli_hosts::read);
        let Some(info) = self.snapshot.repo.clone() else {
            return;
        };
        let (target, cancelled) = (&self.request.target, &self.cancelled);
        if !std::mem::take(&mut self.branches_fresh)
            && let Some(host) = &self.host
        {
            // The branches last read stay when they cannot be read now.
            match ssh_slots::run(target, cancelled, || {
                naming::local_branches(host, &info.main_root, cancelled)
            }) {
                Ok(branches) => self.snapshot.branches = Arc::new(branches),
                Err(error) => tracing::debug!(%error, "Cannot read the branches"),
            }
        }
        for (key, supported) in sources(&info, self.linear_source()) {
            if key.provider == Provider::Linear && !self.linear_due(linear) {
                continue;
            }
            if !self.sync_one(key, supported, token) {
                return;
            }
        }
    }

    /// Syncs Linear alone, now: its key changed, an issue moved or was
    /// created, or its section shows again. A failure's wait still holds.
    pub(super) fn sync_linear_now(&mut self) {
        let Some(info) = self.snapshot.repo.clone() else {
            return;
        };
        if let Some((key, supported)) = sources(&info, self.linear_source())
            .into_iter()
            .find(|(key, _)| key.provider == Provider::Linear)
        {
            self.sync_one(key, supported, None);
        }
    }

    /// Whether Linear syncs in a sync that reads it as `linear` says: its
    /// own interval keeps Linear's complexity budget, which a listing of
    /// hundreds of issues each minute would use up.
    fn linear_due(&self, linear: Linear) -> bool {
        linear == Linear::Now
            || self
                .linear_synced
                .is_none_or(|at| at.elapsed() >= self.request.timing.linear)
    }

    /// Syncs `key` unless it waits out a failure; `false` when cancelled.
    fn sync_one(&mut self, key: SourceKey, supported: bool, token: Option<&Tokens>) -> bool {
        if self.cancelled() {
            return false;
        }
        let now = Instant::now();
        if !supported || self.retry.waits(&key, now) {
            return true;
        }
        if key.provider == Provider::Linear {
            self.linear_synced = Some(now);
        }
        self.set_syncing(&key);
        self.publish();
        // Only this source's own GitHub reply may set its pause: a delay
        // left by a pull request lookup or another source must not apply.
        self.cooldown = None;
        let result = self.sync_source(&key, token);
        // A Linear sync learns its workspace, which keys the source.
        let key = match key.provider {
            Provider::Linear => {
                crate::linear::source(self.linear_workspace.as_deref().unwrap_or(""))
            }
            _ => key,
        };
        let state = match result {
            // The view shows no sync times, so the log keeps them.
            Ok(state) => {
                tracing::debug!(
                    category = "orchestrator_sync",
                    provider = ?key.provider,
                    host = key.host.as_str(),
                    repository = key.repository.as_str(),
                    elapsed_ms = now.elapsed().as_millis(),
                    state = ?state,
                    "Orchestrator source synced"
                );
                self.retry.succeeded(&key);
                state
            }
            Err(Error::Cancelled) => return false,
            Err(error) => {
                let wait = self.retry.failed(&key, &error, self.cooldown.take());
                tracing::warn!(
                    category = "orchestrator_sync",
                    provider = ?key.provider,
                    host = key.host.as_str(),
                    repository = key.repository.as_str(),
                    retry_in_secs = wait.map(|wait| wait.as_secs()),
                    error = %error,
                    "Orchestrator source sync failed"
                );
                SyncState::Failed(Arc::new(error))
            }
        };
        self.set_state(&key, state);
        if let Err(error) = self.reload() {
            self.snapshot.error = Some(Arc::new(error));
        }
        self.publish();
        true
    }

    fn sync_source(&mut self, key: &SourceKey, token: Option<&Tokens>) -> Result<SyncState> {
        let (Some(store), Some(host)) = (self.store.as_mut(), self.host.as_ref()) else {
            return Ok(SyncState::Unsupported);
        };
        let writable = store.access() == Access::ReadWrite;
        match key.provider {
            Provider::Beads => {
                let (items, checkpoint) =
                    ssh_slots::run(&self.request.target, &self.cancelled, || {
                        beads::sync(host, key, &self.cancelled)
                    })?;
                let count = items.len();
                if writable {
                    store.replace_items(key, &items, &checkpoint)?;
                    self.uncached.remove(key);
                } else {
                    self.uncached.insert(key.clone(), items);
                }
                Ok(SyncState::Synced {
                    at: Utc::now(),
                    count,
                })
            }
            Provider::Github => {
                let Some(tokens) = token else {
                    return Ok(SyncState::SignIn);
                };
                let owner = key
                    .repository
                    .split_once('/')
                    .map_or(key.repository.as_str(), |(owner, _)| owner);
                let token = tokens.for_owner(owner);
                let previous = store.checkpoint(key)?;
                // What the last complete sync stored, for reading only the
                // changes since; a read-only store keeps none.
                let cached = match (&previous, writable) {
                    (Some(_), true) => Some(
                        store
                            .items()?
                            .into_iter()
                            .filter(|item| item.key.source == *key)
                            .collect(),
                    ),
                    _ => None,
                };
                // Only while the stored checkpoint is still the one this
                // worker wrote: agent-launcher shares the database and may
                // have stored an older listing since.
                let resume = self.resume.get(key).copied().filter(|resume| {
                    previous
                        .as_ref()
                        .and_then(|checkpoint| checkpoint.updated_at)
                        == Some(resume.oldest())
                });
                let cancelled = &self.cancelled;
                let listing = github::sync(
                    key,
                    token,
                    previous.as_ref(),
                    (cached, resume),
                    &|| cancelled.load(Ordering::Relaxed),
                    &mut self.cooldown,
                )?;
                let count = listing.items.len();
                // Reviews and checks change with every CI run, so they are
                // read again each sync. Failing to read them keeps the last
                // ones rather than failing the listing.
                let statuses = crate::review::fetch(&key.repository, |query, variables| {
                    herdr_github::graphql(
                        "orchestrator",
                        token,
                        query,
                        variables,
                        crate::review::timeout(),
                        || cancelled.load(Ordering::Relaxed),
                        &mut self.cooldown,
                    )
                    .map_err(|error| match error {
                        herdr_github::Error::PullRequest(
                            herdr_github::pull_request::Error::Cancelled,
                        ) => Error::Cancelled,
                        error => Error::Github(Box::new(error)),
                    })
                });
                match statuses {
                    Ok(statuses) => {
                        if writable
                            && let Err(error) =
                                store.replace_pr_statuses(&key.repository, &statuses)
                        {
                            tracing::warn!(%error, "Cannot keep pull request checks");
                        }
                        let mut all = (*self.snapshot.pr_status).clone();
                        all.retain(|(repository, _), _| *repository != key.repository);
                        all.extend(
                            statuses
                                .into_iter()
                                .map(|(number, status)| ((key.repository.clone(), number), status)),
                        );
                        self.snapshot.pr_status = Arc::new(all);
                    }
                    Err(Error::Cancelled) => return Err(Error::Cancelled),
                    Err(error) => tracing::warn!(%error, "Cannot read pull request checks"),
                }
                // A listing that is not stored is no place to resume from.
                self.resume.remove(key);
                if !listing.complete || !writable {
                    self.uncached.insert(key.clone(), listing.items);
                    return Ok(if listing.complete {
                        SyncState::Synced {
                            at: Utc::now(),
                            count,
                        }
                    } else {
                        SyncState::Capped { count }
                    });
                }
                store.replace_items(key, &listing.items, &listing.checkpoint)?;
                self.resume.insert(key.clone(), listing.resume);
                self.uncached.remove(key);
                Ok(SyncState::Synced {
                    at: Utc::now(),
                    count,
                })
            }
            Provider::Gitlab => {
                let scheme = self
                    .snapshot
                    .repo
                    .as_ref()
                    .and_then(|info| info.remote.as_ref())
                    .filter(|remote| remote.source == *key)
                    .map_or(gitlab::Scheme::Https, |remote| {
                        gitlab::Scheme::of_remote(&remote.url)
                    });
                let api = gitlab::Api::resolve(&key.host, scheme);
                if self
                    .gitlab_token
                    .as_ref()
                    .is_none_or(|(instance, _)| *instance != api.instance)
                {
                    let token = gitlab::token(&api.instance);
                    self.gitlab_token = Some((api.instance.clone(), token));
                }
                let token = self
                    .gitlab_token
                    .as_ref()
                    .and_then(|(_, token)| token.as_ref());
                let previous = store.checkpoint(key)?;
                let cancelled = &self.cancelled;
                let listing = gitlab::sync(
                    (key, &api),
                    token,
                    previous.as_ref(),
                    &|| cancelled.load(Ordering::Relaxed),
                    &mut self.cooldown,
                )?;
                let count = listing.items.len();
                if !listing.complete || !writable {
                    self.uncached.insert(key.clone(), listing.items);
                    return Ok(if listing.complete {
                        SyncState::Synced {
                            at: Utc::now(),
                            count,
                        }
                    } else {
                        SyncState::Capped { count }
                    });
                }
                store.replace_items(key, &listing.items, &listing.checkpoint)?;
                self.uncached.remove(key);
                Ok(SyncState::Synced {
                    at: Utc::now(),
                    count,
                })
            }
            Provider::Linear => self.sync_linear(),
        }
    }
}
