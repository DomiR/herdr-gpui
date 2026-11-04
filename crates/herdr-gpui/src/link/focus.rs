//! Resolve a focus link against one coherent Herdr snapshot.

use super::{Below, Focus};
use crate::navigation::OwnedNavigationTarget;
use herdr_client::protocol::ClientShellSnapshot;
use std::collections::HashSet;

pub(crate) mod build;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum What {
    Agent,
    Tab,
    Workspace,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Miss {
    Ambiguous(What),
    Gone(What),
}

pub(crate) fn resolve(
    focus: &Focus,
    snapshot: &ClientShellSnapshot,
) -> Result<OwnedNavigationTarget, Miss> {
    let workspaces: Vec<_> = snapshot
        .workspaces
        .iter()
        .filter(|workspace| workspace_matches(focus, snapshot, workspace))
        .collect();
    if workspaces.is_empty() {
        return Err(Miss::Gone(What::Workspace));
    }

    let workspace_ids: HashSet<&str> = workspaces
        .iter()
        .map(|workspace| workspace.workspace_id.as_str())
        .collect();
    match &focus.tab_or_agent {
        Some(Below::Agent(agent)) => choose(
            snapshot
                .agents
                .iter()
                .filter(|candidate| {
                    candidate.name.as_deref() == Some(agent)
                        && workspace_ids.contains(candidate.workspace_id.as_str())
                })
                .map(|candidate| (candidate.pane_id.as_str(), candidate.pane_id.clone())),
            focus.id.as_deref(),
            What::Agent,
            OwnedNavigationTarget::Pane,
        ),
        Some(Below::Tab(tab)) => choose(
            snapshot
                .tabs
                .iter()
                .filter(|candidate| {
                    candidate.label == *tab
                        && workspace_ids.contains(candidate.workspace_id.as_str())
                })
                .map(|candidate| (candidate.tab_id.as_str(), candidate.tab_id.clone())),
            focus.id.as_deref(),
            What::Tab,
            OwnedNavigationTarget::Tab,
        ),
        None => choose(
            workspaces.into_iter().map(|workspace| {
                (
                    workspace.workspace_id.as_str(),
                    workspace.workspace_id.clone(),
                )
            }),
            focus.id.as_deref(),
            What::Workspace,
            OwnedNavigationTarget::Workspace,
        ),
    }
}

fn workspace_matches(
    focus: &Focus,
    snapshot: &ClientShellSnapshot,
    workspace: &herdr_client::protocol::ClientShellWorkspace,
) -> bool {
    focus
        .workspace
        .as_ref()
        .is_none_or(|label| workspace.label == *label)
        && focus.path.as_ref().is_none_or(|path| {
            crate::palette::launch_root(snapshot, &workspace.workspace_id).is_some_and(|root| {
                let path = path.to_string_lossy();
                normalize_path(root) == normalize_path(&path)
            })
        })
}

fn normalize_path(path: &str) -> &str {
    path.trim_end_matches(['/', '\\'])
}

fn choose<'a>(
    candidates: impl IntoIterator<Item = (&'a str, String)>,
    id_hint: Option<&str>,
    what: What,
    target: impl FnOnce(String) -> OwnedNavigationTarget,
) -> Result<OwnedNavigationTarget, Miss> {
    let candidates: Vec<_> = candidates.into_iter().collect();
    if let Some(id) = id_hint
        && let Some((_, candidate)) = candidates.iter().find(|(candidate, _)| *candidate == id)
    {
        return Ok(target(candidate.clone()));
    }
    match candidates.as_slice() {
        [] => Err(Miss::Gone(what)),
        [(_, candidate)] => Ok(target(candidate.clone())),
        _ => Err(Miss::Ambiguous(what)),
    }
}

#[cfg(test)]
mod tests;
