use super::{Below, Focus};
use crate::{
    link::{Link, validated_local_path},
    navigation::NavigationTarget,
};
use herdr_client::protocol::{ClientShellSnapshot, ClientShellTab, ClientShellWorkspace};

pub(crate) struct Made {
    pub(crate) link: Link,
    pub(crate) label: String,
}

impl Made {
    pub(crate) fn url(&self) -> String {
        self.link.to_url()
    }

    pub(crate) fn markdown(&self) -> String {
        let label = self
            .label
            .replace('\\', "\\\\")
            .replace('[', "\\[")
            .replace(']', "\\]")
            .replace(['\n', '\r'], " ");
        format!("[{label}]({})", self.url())
    }
}

pub(crate) fn link_for(
    snapshot: &ClientShellSnapshot,
    target: NavigationTarget<&str>,
    device: Option<&str>,
) -> Option<Made> {
    let (workspace_id, below, label, id) = match target {
        NavigationTarget::Workspace(workspace_id) => {
            let workspace = workspace(snapshot, workspace_id)?;
            (
                workspace_id,
                None,
                workspace.label.clone(),
                workspace.workspace_id.clone(),
            )
        }
        NavigationTarget::Tab(tab_id) => {
            let tab = snapshot.tabs.iter().find(|tab| tab.tab_id == tab_id)?;
            let (below, label, id) = match tab_below(snapshot, tab) {
                Some((name, pane_id)) => (
                    Below::Agent(name.to_owned()),
                    name.to_owned(),
                    pane_id.to_owned(),
                ),
                None => (
                    Below::Tab(tab.label.clone()),
                    tab.label.clone(),
                    tab_id.to_owned(),
                ),
            };
            (tab.workspace_id.as_str(), Some(below), label, id)
        }
        NavigationTarget::Pane(pane_id) => {
            let pane = snapshot.panes.iter().find(|pane| pane.pane_id == pane_id)?;
            let agent = snapshot
                .agents
                .iter()
                .find(|agent| agent.pane_id == pane.pane_id)
                .and_then(|agent| agent.name.as_ref())?;
            (
                pane.workspace_id.as_str(),
                Some(Below::Agent(agent.clone())),
                agent.clone(),
                pane.pane_id.to_owned(),
            )
        }
    };
    let workspace = workspace(snapshot, workspace_id)?;
    let path = crate::palette::launch_root(snapshot, workspace_id).and_then(validated_local_path);
    let workspace_label =
        (workspace.custom_label || path.is_none()).then(|| workspace.label.clone());
    let device = device
        .filter(|device| *device != crate::endpoint::LOCAL)
        .map(str::to_owned);
    let link = Link::Focus(Focus {
        path,
        workspace: workspace_label,
        tab_or_agent: below,
        id: Some(id),
        device,
    });
    if !matches!(Link::parse(&link.to_url()), Ok(parsed) if parsed == link) {
        return None;
    }
    Some(Made { link, label })
}

fn workspace<'a>(snapshot: &'a ClientShellSnapshot, id: &str) -> Option<&'a ClientShellWorkspace> {
    snapshot
        .workspaces
        .iter()
        .find(|workspace| workspace.workspace_id == id)
}

fn tab_below<'a>(
    snapshot: &'a ClientShellSnapshot,
    tab: &ClientShellTab,
) -> Option<(&'a str, &'a str)> {
    if tab.custom_label {
        return None;
    }
    let mut named = snapshot
        .agents
        .iter()
        .filter(|agent| agent.tab_id == tab.tab_id && agent.workspace_id == tab.workspace_id)
        .filter_map(|agent| {
            agent
                .name
                .as_deref()
                .map(|name| (name, agent.pane_id.as_str()))
        });
    let name = named.next()?;
    named.next().is_none().then_some(name)
}
