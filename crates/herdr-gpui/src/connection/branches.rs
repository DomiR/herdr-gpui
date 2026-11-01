//! Each workspace's branch as its checkout has it now, for the owned local
//! daemon. Herdr reads a workspace's branch when the space opens and again
//! only while its own sidebar shows branches or a pane's directory changes,
//! so after a `git switch` or `git branch -m` it keeps naming the old one,
//! and every lookup keyed by it (the pull request, the uncommitted mark)
//! finds no worktree on it. Read from Git's files on the bridge's reader
//! thread, so it starts no process and never runs on the UI thread.

use herdr_client::{ClientEvent, protocol::ClientShellWorkspace};
use std::{
    collections::HashMap,
    path::Path,
    sync::Arc,
    time::{Duration, Instant},
};

/// How long a read answers for its directory: snapshots can come several a
/// second, and Herdr's own refresh, when on, is no faster.
const FRESH: Duration = Duration::from_secs(2);

/// The last read per workspace directory and repository.
#[derive(Default)]
pub(super) struct Branches {
    read: HashMap<(String, Option<String>), (Instant, Option<String>)>,
}

impl Branches {
    /// `event`, with each workspace's branch replaced by the one its checkout
    /// has out when the two differ. A detached checkout, or one these files
    /// do not settle, keeps the daemon's.
    pub(super) fn apply(&mut self, event: ClientEvent, now: Instant) -> ClientEvent {
        let ClientEvent::Snapshot(mut snapshot) = event else {
            return event;
        };
        let live: Vec<(usize, String)> = snapshot
            .workspaces
            .iter()
            .enumerate()
            .filter_map(|(index, workspace)| {
                let branch = self.branch(workspace, now)?;
                (workspace.branch.as_deref() != Some(branch.as_str())).then_some((index, branch))
            })
            .collect();
        self.read
            .retain(|_, (at, _)| now.duration_since(*at) < FRESH);
        if !live.is_empty() {
            let snapshot = Arc::make_mut(&mut snapshot);
            for (index, branch) in live {
                snapshot.workspaces[index].branch = Some(branch);
            }
        }
        ClientEvent::Snapshot(snapshot)
    }

    fn branch(&mut self, workspace: &ClientShellWorkspace, now: Instant) -> Option<String> {
        let key = workspace.worktree.as_ref().map(|tree| tree.key.clone());
        let entry = (workspace.new_workspace_cwd.clone(), key);
        if let Some((at, branch)) = self.read.get(&entry)
            && now.duration_since(*at) < FRESH
        {
            return branch.clone();
        }
        let branch = read(&entry.0, entry.1.as_deref());
        self.read.insert(entry, (now, branch.clone()));
        branch
    }
}

/// The branch checked out where `cwd` is, when it is a checkout of the
/// repository `key` names; with no key, any repository's, as its lookups
/// then verify the directory themselves. A shell that left the repository
/// Herdr keeps for the workspace no longer answers for it.
fn read(cwd: &str, key: Option<&str>) -> Option<String> {
    if cwd.is_empty() || !Path::new(cwd).is_absolute() {
        return None;
    }
    let found = herdr_github::checkout::checkout_at(cwd)?;
    if let Some(key) = key
        && Path::new(key).canonicalize().ok()? != found.common
    {
        return None;
    }
    found.branch
}

#[cfg(test)]
mod tests;
