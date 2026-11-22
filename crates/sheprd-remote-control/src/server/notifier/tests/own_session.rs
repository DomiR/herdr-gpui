//! A daemon pushes only for its own session's panes. A herdr-core lists
//! other machines' spaces beside this one's; were the hub to mirror them,
//! every linked computer would alert for the same agent and the phone
//! would show it twice. This locks the whole path, from the core's
//! document to the alert, so such a change cannot add a second sender
//! unnoticed.

use super::*;
use crate::hub::project_spaces;
use crate::push::Privacy;

/// Long past the time a terminal agent's status takes to settle.
const SETTLED: u64 = 60_000;
use serde_json::{Value, json};
use sheprd_core_client::wire::Terminal;

fn space(id: &str, name: &str, machine: Option<&str>, terminal: &str) -> Value {
    json!({"id": id, "folder": "/f", "name": name, "machine": machine, "tabs": [
        {"id": format!("{id}-tab"), "name": "one",
         "root": {"type": "block", "content": {"terminal": terminal}}}
    ]})
}

fn records(state: &str) -> Vec<Terminal> {
    ["t1", "t2", "box/t3"]
        .map(|address| {
            serde_json::from_value(json!({"address": address, "cols": 80, "rows": 24,
                "cwd": "/code/app", "state": "live", "pid": 123,
                "status": {"state": state, "app": "claude"}}))
            .unwrap()
        })
        .to_vec()
}

fn layout(state: &str) -> Layout {
    let spaces = json!({"spaces": [
        space("desk", "desk", None, "t1"),
        space("remote", "other machine", Some("box"), "t2"),
        space("box/sp", "prefixed", None, "box/t3"),
    ]});
    project_spaces("core-boot", spaces, &records(state))
}

/// The alerts every agent asking at once calls for.
fn alerts() -> Vec<Notification> {
    let mut agents = TerminalAgents::default();
    let working = layout("working");
    let names = agent_names(&working);
    agents.step(terminal_agents(&working, &names), 0);
    let asking = layout("blocked");
    let names = agent_names(&asking);
    let seen = terminal_agents(&asking, &names);
    agents.step(seen.iter().copied(), 1);
    agents
        .step(seen.iter().copied(), SETTLED)
        .iter()
        .filter_map(|moved| {
            let agent = seen.iter().find(|agent| agent.chat.pane == moved.pane)?;
            decide_terminal(agent, moved, Privacy::Generic)
        })
        .collect()
}

#[test]
fn another_machines_space_never_alerts() {
    let alerts = alerts();
    assert_eq!(alerts.len(), 1, "only this session's agent: {alerts:?}");
    assert!(alerts[0].body.starts_with("desk"));
    let layout = layout("blocked");
    let names = agent_names(&layout);
    let panes: Vec<&str> = terminal_agents(&layout, &names)
        .iter()
        .map(|agent| agent.chat.pane)
        .collect();
    assert_eq!(panes, [alerts[0].pane.as_str()]);
}
