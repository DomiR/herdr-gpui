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

/// The shipped activity bar app loads, declares its icon, and draws.
#[test]
fn the_shipped_dashboard_app_works() {
    let sources = Sources::new([
        (
            "mod.toml".to_owned(),
            include_bytes!("../../../mods/dashboard/mod.toml").to_vec(),
        ),
        (
            "main.luau".to_owned(),
            include_bytes!("../../../mods/dashboard/main.luau").to_vec(),
        ),
        (
            "icon.svg".to_owned(),
            include_bytes!("../../../mods/dashboard/icon.svg").to_vec(),
        ),
    ])
    .unwrap();
    let activity = sources.manifest.activity.clone().unwrap();
    assert_eq!(activity.title, "Dashboard");
    assert!(
        sources
            .icon(&activity.icon)
            .unwrap()
            .contains("currentColor")
    );
    let now = SystemTime::now();
    let mut module = Mod::load(&sources, Limits::default(), now).unwrap();
    let tree = module.view(&State::default(), now).unwrap();
    assert!(matches!(tree, Node::Column(_)));
    module.update(&Data::from("reviewed"), now).unwrap();
}
