use super::*;
use herdr_mods::{Agent, AgentStatus, Limits, Mod, Sources};
use std::time::SystemTime;

/// The example shipped beside the app loads, draws, and answers its command.
#[test]
fn the_shipped_example_works() {
    let sources = Sources::new([
        (
            "mod.toml".to_owned(),
            include_bytes!("../../../mods/agents/mod.toml").to_vec(),
        ),
        (
            "main.luau".to_owned(),
            include_bytes!("../../../mods/agents/main.luau").to_vec(),
        ),
    ])
    .unwrap();
    let now = SystemTime::now();
    let mut module = Mod::load(&sources, Limits::default(), now).unwrap();
    let quiet = module.view(&State::default(), now).unwrap();
    let Node::Row(quiet) = quiet else {
        panic!("expected a row, got {quiet:?}");
    };
    assert_eq!(text(&quiet.children[0]), "No agents busy");

    let blocked = State {
        agents: vec![Agent {
            id: "p1".into(),
            host: "local".into(),
            workspace: "w".into(),
            name: "claude".into(),
            status: AgentStatus::Blocked,
        }],
        ..State::default()
    };
    module.update(&Data::from("command:names"), now).unwrap();
    let busy = module.view(&blocked, now).unwrap();
    let Node::Tooltip(tooltip) = busy else {
        panic!("expected a tooltip, got {busy:?}");
    };
    let Node::Row(row) = *tooltip.child else {
        panic!("expected a row");
    };
    assert!(matches!(&row.children[0], Node::Badge(badge) if badge.text == "1 waiting"));
    assert_eq!(text(&row.children[1]), "claude");
}
