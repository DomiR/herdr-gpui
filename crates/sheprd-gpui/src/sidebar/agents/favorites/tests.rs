use super::*;
use crate::sidebar::{CoreAgent, layout_tests};
use core::prelude::v1::test;
use sheprd_client::protocol::ClientShellAgent;
use sheprd_remote_control::control::favorites::core_pane;
use sheprd_session::{Agent, Session};

fn agents(panes: &[&str]) -> Vec<ClientShellAgent> {
    let snapshot = layout_tests::snapshot(1);
    panes
        .iter()
        .map(|pane| ClientShellAgent {
            pane_id: (*pane).into(),
            ..snapshot.agents[0].clone()
        })
        .collect()
}

fn core_agent(terminal: &str) -> Agent {
    Agent {
        focused: false,
        identity: Some("claude".into()),
        name: "claude".into(),
        pane: terminal.into(),
        status: sheprd_backend::AgentStatus::Idle,
        tab: "1".into(),
        workspace: "kept".into(),
    }
}

fn ids<'a>(rows: &[(usize, Listed<'a>)]) -> Vec<(usize, &'a str)> {
    rows.iter()
        .map(|(index, agent)| (*index, agent.id()))
        .collect()
}

#[test]
fn favorites_go_first_and_each_part_keeps_its_order() {
    let snapshot = layout_tests::snapshot(1);
    let agents = agents(&["p1", "p2", "p3", "p4"]);
    let rows: Vec<_> = agents
        .iter()
        .enumerate()
        .map(|(index, agent)| {
            (
                index % 2,
                Listed::Herdr1 {
                    agent,
                    snapshot: &snapshot,
                },
            )
        })
        .collect();
    let sets = crate::agent_favorites::Sets::starred(&[("host-1", &["p2", "p4"])]);
    let device = |index: usize| if index == 1 { "host-1" } else { "host-0" };
    let sorted = favorites_first(rows, |index, agent| {
        agent
            .favorite_key()
            .is_some_and(|key| sets.contains(device(index), &key))
    });
    assert_eq!(ids(&sorted), [(1, "p2"), (1, "p4"), (0, "p1"), (0, "p3")]);
}

#[test]
fn a_herdr_2_agent_is_kept_by_its_cores_name_for_the_terminal() {
    let session = Session::default();
    let agent = core_agent("t7");
    let listed = |core| {
        Listed::Core(CoreAgent {
            agent: &agent,
            core,
            session: &session,
            snapshot: None,
            info: None,
            focused: false,
        })
    };
    assert_eq!(
        listed(Some("core-1")).favorite_key(),
        Some(core_pane("core-1", "t7")),
        "the id the host's daemon shows phones"
    );
    assert_eq!(listed(None).favorite_key(), None, "no core, no key");

    let snapshot = layout_tests::snapshot(1);
    let herdr_1 = agents(&["p9"]);
    let listed = Listed::Herdr1 {
        agent: &herdr_1[0],
        snapshot: &snapshot,
    };
    assert_eq!(listed.favorite_key().as_deref(), Some("p9"));
}
