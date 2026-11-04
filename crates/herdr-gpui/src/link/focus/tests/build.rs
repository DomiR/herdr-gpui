use super::*;
use crate::{
    link::focus::build::{Made, link_for},
    link::{Below, Focus, Link},
    navigation::{NavigationTarget, OwnedNavigationTarget},
};
use herdr_doc::markdown::{Style, inline, lines, link_url};
use std::path::Path;

fn made(snapshot: &ClientShellSnapshot, target: NavigationTarget<&str>) -> Made {
    link_for(snapshot, target, None).expect("fixture target makes a link")
}

fn focus_link(made: &Made) -> &Focus {
    match &made.link {
        Link::Focus(focus) => focus,
        _ => panic!("builder did not make a focus link"),
    }
}

#[test]
fn workspace_uses_its_first_pane_path() {
    let snapshot = snapshot();
    let made = made(&snapshot, NavigationTarget::Workspace("w0"));
    let focus = focus_link(&made);
    assert_eq!(focus.path.as_deref(), Some(Path::new("/checkouts/notes/")));
    assert_eq!(focus.workspace, None);
    assert_eq!(focus.id.as_deref(), Some("w0"));
    assert_eq!(made.label, "Notes");
}

#[test]
fn custom_label_workspace_keeps_its_label() {
    let mut snapshot = snapshot();
    snapshot.workspaces[0].custom_label = true;
    let made = made(&snapshot, NavigationTarget::Workspace("w0"));
    let focus = focus_link(&made);
    assert_eq!(focus.workspace.as_deref(), Some("Notes"));
    assert!(focus.path.is_some());
}

#[test]
fn custom_tab_uses_the_tab_label() {
    let snapshot = snapshot();
    let made = made(&snapshot, NavigationTarget::Tab("w0:t1"));
    let focus = focus_link(&made);
    assert_eq!(focus.tab_or_agent, Some(Below::Tab("build".into())));
    assert_eq!(focus.id.as_deref(), Some("w0:t1"));
}

#[test]
fn one_named_agent_on_an_ordinary_tab_gets_the_agent_link() {
    let mut snapshot = snapshot();
    snapshot.tabs[0].custom_label = false;
    let made = made(&snapshot, NavigationTarget::Tab("w0:t1"));
    let focus = focus_link(&made);
    assert_eq!(focus.tab_or_agent, Some(Below::Agent("reviewer".into())));
    assert_eq!(focus.id.as_deref(), Some("w0:p1"));
    assert_eq!(made.label, "reviewer");
}

#[test]
fn two_named_agents_keep_the_tab_link() {
    let mut snapshot = snapshot();
    snapshot.tabs[0].custom_label = false;
    snapshot.agents.push(ClientShellAgent {
        pane_id: "w0:p2".into(),
        workspace_id: "w0".into(),
        tab_id: "w0:t1".into(),
        name: Some("builder".into()),
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
    });
    let made = made(&snapshot, NavigationTarget::Tab("w0:t1"));
    let focus = focus_link(&made);
    assert_eq!(focus.tab_or_agent, Some(Below::Tab("build".into())));
}

#[test]
fn unnamed_pane_does_not_make_a_link() {
    let mut snapshot = snapshot();
    snapshot.agents[0].name = None;
    assert!(link_for(&snapshot, NavigationTarget::Pane("w0:p1"), None).is_none());
}

#[test]
fn device_is_only_emitted_for_a_non_local_endpoint() {
    let snapshot = snapshot();
    let local = link_for(
        &snapshot,
        NavigationTarget::Workspace("w0"),
        Some(crate::endpoint::LOCAL),
    )
    .unwrap();
    let remote = link_for(&snapshot, NavigationTarget::Workspace("w0"), Some("box")).unwrap();
    assert_eq!(focus_link(&local).device, None);
    assert_eq!(focus_link(&remote).device.as_deref(), Some("box"));
}

#[test]
fn made_links_resolve_to_their_targets() {
    let snapshot = snapshot();
    for (target, expected) in [
        (
            NavigationTarget::Workspace("w0"),
            OwnedNavigationTarget::Workspace("w0".into()),
        ),
        (
            NavigationTarget::Tab("w0:t1"),
            OwnedNavigationTarget::Tab("w0:t1".into()),
        ),
        (
            NavigationTarget::Pane("w0:p1"),
            OwnedNavigationTarget::Pane("w0:p1".into()),
        ),
    ] {
        let made = link_for(&snapshot, target, None).unwrap();
        let Link::Focus(focus) = made.link else {
            panic!("not a focus link");
        };
        assert_eq!(resolve(&focus, &snapshot), Ok(expected));
    }
}

#[test]
fn markdown_escapes_labels_and_keeps_the_form_url_whole() {
    let snapshot = snapshot();
    let made = made(&snapshot, NavigationTarget::Workspace("w0"));
    let made = Made {
        link: made.link,
        label: "A[\\]\nB".into(),
    };
    assert_eq!(
        made.markdown(),
        "[A\\[\\\\\\] B](herdr-gpui://focus?path=%2Fcheckouts%2Fnotes%2F&id=w0)"
    );
    let markdown = made.markdown();
    let line = &lines(&markdown)[0];
    let span = inline(&markdown, line)
        .into_iter()
        .find(|span| matches!(span.style, Style::Link { .. }))
        .expect("markdown parser found the link");
    let url = made.url();
    assert_eq!(link_url(&markdown, &span), Some(url.as_str()));
}
