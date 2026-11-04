#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;
use crate::link::{Below, Focus};
use crate::navigation::OwnedNavigationTarget;
use herdr_client::protocol::{
    AgentStatus, ClientShellAgent, ClientShellPane, ClientShellSnapshot, ClientShellTab,
};

mod build;

fn snapshot() -> ClientShellSnapshot {
    let mut snapshot = crate::sidebar::layout_tests::snapshot(2);
    snapshot.workspaces[0].label = "Notes".into();
    snapshot.workspaces[1].label = "Notes".into();
    snapshot.tabs = vec![
        ClientShellTab {
            tab_id: "w0:t1".into(),
            workspace_id: "w0".into(),
            number: 1,
            label: "build".into(),
            custom_label: true,
            zoomed: false,
            focused: false,
            agent_status: AgentStatus::Idle,
        },
        ClientShellTab {
            tab_id: "w1:t1".into(),
            workspace_id: "w1".into(),
            number: 1,
            label: "build".into(),
            custom_label: true,
            zoomed: false,
            focused: false,
            agent_status: AgentStatus::Idle,
        },
    ];
    snapshot.panes = vec![
        ClientShellPane {
            pane_id: "w0:p1".into(),
            workspace_id: "w0".into(),
            tab_id: "w0:t1".into(),
            label: None,
            cwd: Some("/checkouts/notes/".into()),
            foreground_cwd: None,
            focused: false,
            right_click_passthrough: false,
        },
        ClientShellPane {
            pane_id: "w1:p1".into(),
            workspace_id: "w1".into(),
            tab_id: "w1:t1".into(),
            label: None,
            cwd: Some("/checkouts/notes-two".into()),
            foreground_cwd: None,
            focused: false,
            right_click_passthrough: false,
        },
    ];
    snapshot.agents = vec![
        ClientShellAgent {
            pane_id: "w0:p1".into(),
            workspace_id: "w0".into(),
            tab_id: "w0:t1".into(),
            name: Some("reviewer".into()),
            display_agent: None,
            agent: None,
            title: None,
            terminal_title: None,
            terminal_title_stripped: None,
            agent_status: AgentStatus::Working,
            state_change_seq: 0,
            state_labels: Vec::new(),
            tokens: Vec::new(),
            focused: false,
        },
        ClientShellAgent {
            pane_id: "w1:p1".into(),
            workspace_id: "w1".into(),
            tab_id: "w1:t1".into(),
            name: Some("reviewer".into()),
            display_agent: None,
            agent: None,
            title: None,
            terminal_title: None,
            terminal_title_stripped: None,
            agent_status: AgentStatus::Working,
            state_change_seq: 0,
            state_labels: Vec::new(),
            tokens: Vec::new(),
            focused: false,
        },
    ];
    snapshot
}

fn focus() -> Focus {
    Focus {
        path: None,
        workspace: None,
        tab_or_agent: None,
        id: None,
        device: None,
    }
}

#[test]
fn path_matches_after_trailing_separator_is_trimmed() {
    let mut focus = focus();
    focus.path = Some("/checkouts/notes".into());
    assert_eq!(
        resolve(&focus, &snapshot()),
        Ok(OwnedNavigationTarget::Workspace("w0".into()))
    );
}

#[test]
fn workspace_label_matches_exactly() {
    let mut focus = focus();
    focus.workspace = Some("Notes".into());
    assert_eq!(
        resolve(&focus, &snapshot()),
        Err(Miss::Ambiguous(What::Workspace))
    );
    focus.id = Some("w1".into());
    assert_eq!(
        resolve(&focus, &snapshot()),
        Ok(OwnedNavigationTarget::Workspace("w1".into()))
    );
}

#[test]
fn tab_label_can_be_unique_within_two_workspaces() {
    let mut snapshot = snapshot();
    snapshot.tabs[1].label = "test".into();
    let mut focus = focus();
    focus.tab_or_agent = Some(Below::Tab("build".into()));
    focus.workspace = Some("Notes".into());
    assert_eq!(
        resolve(&focus, &snapshot),
        Ok(OwnedNavigationTarget::Tab("w0:t1".into()))
    );
}

#[test]
fn id_hint_breaks_a_tab_label_tie() {
    let mut focus = focus();
    focus.tab_or_agent = Some(Below::Tab("build".into()));
    focus.workspace = Some("Notes".into());
    focus.id = Some("w1:t1".into());
    assert_eq!(
        resolve(&focus, &snapshot()),
        Ok(OwnedNavigationTarget::Tab("w1:t1".into()))
    );
}

#[test]
fn stale_id_does_not_override_a_unique_label() {
    let mut focus = focus();
    focus.tab_or_agent = Some(Below::Tab("build".into()));
    focus.workspace = Some("Notes".into());
    focus.path = Some("/checkouts/notes".into());
    focus.id = Some("stale".into());
    assert_eq!(
        resolve(&focus, &snapshot()),
        Ok(OwnedNavigationTarget::Tab("w0:t1".into()))
    );
}

#[test]
fn ambiguous_and_gone_targets_are_not_picked_by_order() {
    let mut focus = focus();
    focus.tab_or_agent = Some(Below::Tab("build".into()));
    focus.workspace = Some("Notes".into());
    assert_eq!(
        resolve(&focus, &snapshot()),
        Err(Miss::Ambiguous(What::Tab))
    );

    focus.workspace = Some("Missing".into());
    assert_eq!(
        resolve(&focus, &snapshot()),
        Err(Miss::Gone(What::Workspace))
    );

    focus.workspace = Some("Notes".into());
    focus.tab_or_agent = Some(Below::Tab("test".into()));
    assert_eq!(resolve(&focus, &snapshot()), Err(Miss::Gone(What::Tab)));
}

#[test]
fn agent_links_work_with_and_without_workspace_keys() {
    let mut focus = focus();
    focus.tab_or_agent = Some(Below::Agent("reviewer".into()));
    focus.id = Some("w1:p1".into());
    assert_eq!(
        resolve(&focus, &snapshot()),
        Ok(OwnedNavigationTarget::Pane("w1:p1".into()))
    );

    focus.workspace = Some("Notes".into());
    assert_eq!(
        resolve(&focus, &snapshot()),
        Ok(OwnedNavigationTarget::Pane("w1:p1".into()))
    );

    focus.id = None;
    assert_eq!(
        resolve(&focus, &snapshot()),
        Err(Miss::Ambiguous(What::Agent))
    );
}
