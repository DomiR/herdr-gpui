//! Branch names as agent-launcher gives them, `agent/<name>-<hash>`: the
//! item's identifier and title, then the first eight hex digits of a UUIDv5
//! of its canonical key. The hash makes a branch name the item's own, so
//! work on an item is found from its branch alone: a Herdr workspace on the
//! branch, with or without an agent, or the branch in the repository, even
//! when no database recorded the run, as on a machine that never had one.
//!
//! A tracker may name the branch itself, as Linear's `branchName` does. Such
//! a branch has no hash, so a Linear issue's branch is found by the issue's
//! identifier in it instead, as in `dominique/eng-12-fix`.

use super::{Backend, Item, Owner, Provider, Run, RunState, model::Workspace, prompt};
use crate::{orchestrator::view::LiveAgent, teleport::Host};
use chrono::{DateTime, Utc};
use herdr_client::{protocol::AgentStatus, shell_quote};
use std::{
    collections::{HashMap, HashSet},
    sync::atomic::AtomicBool,
};

/// The prefix every branch named here has.
const PREFIX: &str = "agent/";
/// Hex digits of the key's hash a branch ends with.
const HASH: usize = 8;
/// The most branches read from a repository.
const MAX_BRANCHES: usize = 500;

/// The branch for a new checkout of `item`: the one its tracker names when
/// git accepts it, else agent-launcher's.
pub(crate) fn branch(item: &Item) -> String {
    match &item.branch {
        Some(branch) if prompt::valid_branch(branch) => branch.clone(),
        _ => launcher_branch(item),
    }
}

/// agent-launcher's branch for a new checkout of `item`.
fn launcher_branch(item: &Item) -> String {
    let name = workspace_name(&format!("{}-{}", item.identifier, item.title));
    format!(
        "{PREFIX}{name}-{}",
        &key_hash(&item.key.canonical())[..HASH]
    )
}

/// The branch for a read-only review of pull request `item`: its own
/// branch with `review-` before the name, so it is found as the PR's too.
pub(crate) fn review_branch(item: &Item) -> String {
    launcher_branch(item).replacen(PREFIX, &format!("{PREFIX}review-"), 1)
}

/// agent-launcher's `stable_hash`: a UUIDv5 of `key`, as 32 hex digits.
fn key_hash(key: &str) -> String {
    uuid::Uuid::new_v5(&uuid::Uuid::NAMESPACE_URL, key.as_bytes())
        .simple()
        .to_string()
}

/// agent-launcher's `sanitize_workspace_name`: lowercase letters and digits
/// joined by single dashes, at most 64 characters.
fn workspace_name(value: &str) -> String {
    let mut name = String::new();
    let mut separator = false;
    for c in value.chars().flat_map(char::to_lowercase) {
        if c.is_ascii_alphanumeric() {
            name.push(c);
            separator = false;
        } else if !separator && !name.is_empty() {
            name.push('-');
            separator = true;
        }
        if name.len() >= 64 {
            break;
        }
    }
    while name.ends_with('-') {
        name.pop();
    }
    if name.is_empty() {
        "workspace".into()
    } else {
        name
    }
}

/// The key hash a branch named here ends with.
fn branch_hash(branch: &str) -> Option<&str> {
    let (_, hash) = branch.strip_prefix(PREFIX)?.rsplit_once('-')?;
    (hash.len() == HASH
        && hash
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)))
    .then_some(hash)
}

/// The issue identifiers `branch` holds as tokens, uppercased: a letter and
/// letters or digits, a dash, and every digit after it, not preceded by a
/// letter or digit. `dominique/eng-12-fix` holds `ENG-12`; `xeng-12` and
/// `eng-123` do not.
fn identifiers(branch: &str) -> impl Iterator<Item = String> + '_ {
    let bytes = branch.as_bytes();
    let mut at = 0;
    std::iter::from_fn(move || {
        while at < bytes.len() {
            let start = at;
            let boundary = start == 0 || !bytes[start - 1].is_ascii_alphanumeric();
            at += 1;
            if !boundary || !bytes[start].is_ascii_alphabetic() {
                continue;
            }
            while at < bytes.len() && bytes[at].is_ascii_alphanumeric() {
                at += 1;
            }
            if bytes.get(at) != Some(&b'-') {
                continue;
            }
            let digits = at + 1;
            let mut end = digits;
            while end < bytes.len() && bytes[end].is_ascii_digit() {
                end += 1;
            }
            if end == digits {
                continue;
            }
            at = end;
            return Some(branch[start..end].to_ascii_uppercase());
        }
        None
    })
}

/// Whether `branch` could name an item: by its hash, or by an identifier.
fn names_item(branch: &str) -> bool {
    branch_hash(branch).is_some() || identifiers(branch).next().is_some()
}

/// A Herdr workspace on some branch, on a connected host.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LiveBranch {
    /// `None` on this machine, else the SSH destination.
    pub(crate) host: Option<String>,
    pub(crate) session: Option<String>,
    pub(crate) workspace_id: String,
    pub(crate) branch: String,
}

pub(super) fn workspace_run_id(
    host: &Option<String>,
    session: &Option<String>,
    workspace: &str,
) -> String {
    format!("branch:{host:?}:{session:?}:{workspace}")
}

/// Runs read off branches for items no recorded run already covers: one per
/// Herdr workspace on an item's branch, working when an agent there is, and
/// one per repository branch no workspace shows. They are never written; no
/// application owns them, so they have no controls beyond opening.
pub(crate) fn found_runs(
    items: &[Item],
    recorded: &[Run],
    workspaces: &[LiveBranch],
    branches: &[String],
    agents: &[LiveAgent],
) -> Vec<Run> {
    let by_hash: HashMap<String, &Item> = items
        .iter()
        .map(|item| (key_hash(&item.key.canonical())[..HASH].to_owned(), item))
        .collect();
    let by_identifier: HashMap<&str, &Item> = items
        .iter()
        .filter(|item| item.key.source.provider == Provider::Linear)
        .map(|item| (item.identifier.as_str(), item))
        .collect();
    // The hash names an item outright, so it wins over an identifier.
    let item_of = |branch: &str| {
        branch_hash(branch)
            .and_then(|hash| by_hash.get(hash).copied())
            .or_else(|| identifiers(branch).find_map(|id| by_identifier.get(id.as_str()).copied()))
    };
    let known = |branch: &str| {
        recorded.iter().any(|run| {
            run.workspace
                .as_ref()
                .is_some_and(|workspace| workspace.branch == branch)
        })
    };
    let mut found = Vec::new();
    for live in workspaces {
        let Some(item) = item_of(&live.branch).filter(|_| !known(&live.branch)) else {
            continue;
        };
        let agent = agents
            .iter()
            .filter(|agent| {
                agent.host == live.host
                    && agent.session == live.session
                    && agent.workspace_id == live.workspace_id
            })
            .map(|agent| agent.status)
            .max_by_key(|status| urgency(*status));
        found.push(run(
            item,
            workspace_run_id(&live.host, &live.session, &live.workspace_id),
            Workspace {
                backend: Backend::Herdr,
                id: live.workspace_id.clone(),
                host: live.host.clone(),
                path: None,
                branch: live.branch.clone(),
            },
            agent,
        ));
    }
    for branch in branches {
        let shown = workspaces.iter().any(|live| &live.branch == branch);
        let Some(item) = item_of(branch).filter(|_| !shown && !known(branch)) else {
            continue;
        };
        found.push(run(
            item,
            format!("branch:{branch}"),
            Workspace {
                backend: Backend::Herdr,
                // No workspace shows it: there is nothing to open.
                id: String::new(),
                host: None,
                path: None,
                branch: branch.clone(),
            },
            None,
        ));
    }
    found
}

/// Which of a workspace's agents speaks for it: the one needing the most.
fn urgency(status: AgentStatus) -> u8 {
    match status {
        AgentStatus::Blocked => 4,
        AgentStatus::Working => 3,
        AgentStatus::Idle => 2,
        AgentStatus::Done => 1,
        AgentStatus::Unknown => 0,
    }
}

fn run(item: &Item, id: String, workspace: Workspace, agent: Option<AgentStatus>) -> Run {
    // When the work began is unknown; the item's last change stands in.
    let at = item.updated_at.unwrap_or(DateTime::<Utc>::UNIX_EPOCH);
    Run {
        id,
        item_key: item.key.canonical(),
        workspace: Some(workspace),
        agent: if agent.is_some() { "agent" } else { "no agent" }.into(),
        model: None,
        state: match agent {
            Some(_) => RunState::Running,
            None => RunState::Cancelled,
        },
        message: None,
        session_id: None,
        started_at: at,
        updated_at: at,
        owner: Owner::Branch,
    }
}

/// The repository's branches that could name an item, read on its host:
/// those named here first, then the most recently committed to that hold an
/// identifier, such as a Linear issue's own branch.
pub(crate) fn local_branches(
    host: &Host,
    main_root: &str,
    cancelled: &AtomicBool,
) -> Option<Vec<String>> {
    let root = shell_quote(main_root);
    let body = format!(
        "git -C {root} for-each-ref --count={MAX_BRANCHES} --format='%(refname:short)' refs/heads/{PREFIX}\n\
         git -C {root} for-each-ref --count={MAX_BRANCHES} --sort=-committerdate --format='%(refname:short)' refs/heads/\n",
    );
    let output = host.capture(&body, cancelled).ok()?;
    let text = String::from_utf8_lossy(&output);
    let mut seen = HashSet::new();
    Some(
        text.lines()
            .map(str::trim)
            .filter(|branch| names_item(branch) && seen.insert(*branch))
            .map(str::to_owned)
            .collect(),
    )
}

#[cfg(test)]
pub(super) fn hash_of(item: &Item) -> String {
    key_hash(&item.key.canonical())[..HASH].to_owned()
}
