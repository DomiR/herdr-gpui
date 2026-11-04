use super::*;
use crate::{config::DocAgentConfig, doc_link};
use std::time::Duration;

#[test]
fn defaults_are_claude_with_a_25_second_debounce() {
    let config = DocAgentConfig::default();
    assert_eq!(config.agent_command, ["claude"]);
    assert_eq!(config.debounce(), Duration::from_secs(25));
    assert!(config.args().is_empty());
    assert_eq!(config.kind(), crate::teleport::AgentKind::Claude);
    assert_eq!(config.clone().checked().unwrap(), config);
}

#[test]
fn the_doc_agent_table_parses_through_the_full_config() {
    let config = Config::parse(
        r#"
[doc_agent]
agent_command = ["codex", "--profile", "work"]
debounce_seconds = 42
"#,
    )
    .unwrap();
    assert_eq!(
        config.doc_agent.agent_command,
        ["codex", "--profile", "work"]
    );
    assert_eq!(config.doc_agent.debounce(), Duration::from_secs(42));
    assert_eq!(config.doc_agent.args(), ["--profile", "work"]);
    assert_eq!(config.doc_agent.kind(), crate::teleport::AgentKind::Codex);
}

#[test]
fn checked_rejects_each_command_and_debounce_failure() {
    let cases: [Vec<String>; 5] = [
        Vec::new(),
        vec!["unknown".into()],
        (0..65).map(|_| "claude".into()).collect(),
        vec!["claude".into(), "x".repeat(8190)],
        vec!["claude".into(), "bad\narg".into()],
    ];
    assert!(matches!(
        DocAgentConfig {
            agent_command: cases[0].clone(),
            debounce_seconds: 25,
        }
        .checked(),
        Err(doc_link::Error::AgentCommand { .. })
    ));
    assert!(matches!(
        DocAgentConfig {
            agent_command: cases[1].clone(),
            debounce_seconds: 25,
        }
        .checked(),
        Err(doc_link::Error::AgentCommand { .. })
    ));
    assert!(matches!(
        DocAgentConfig {
            agent_command: cases[2].clone(),
            debounce_seconds: 25,
        }
        .checked(),
        Err(doc_link::Error::AgentCommand { .. })
    ));
    assert!(matches!(
        DocAgentConfig {
            agent_command: cases[3].clone(),
            debounce_seconds: 25,
        }
        .checked(),
        Err(doc_link::Error::AgentCommand { .. })
    ));
    assert!(matches!(
        DocAgentConfig {
            agent_command: cases[4].clone(),
            debounce_seconds: 25,
        }
        .checked(),
        Err(doc_link::Error::AgentCommand { .. })
    ));
    for seconds in [0, 301] {
        assert!(matches!(
            DocAgentConfig {
                agent_command: vec!["claude".into()],
                debounce_seconds: seconds,
            }
            .checked(),
            Err(doc_link::Error::Debounce { seconds: value }) if value == seconds
        ));
    }
}
