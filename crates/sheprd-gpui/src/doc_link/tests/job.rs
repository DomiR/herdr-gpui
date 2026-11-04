use super::*;
use crate::teleport::AgentKind;
#[cfg(unix)]
use std::cell::Cell;
use std::{collections::VecDeque, path::Path, sync::atomic::AtomicBool, time::Duration};

fn tab_created(terminal_id: Option<&str>) -> Vec<u8> {
    serde_json::json!({
        "result": {
            "tab": {"tab_id": "tab-1"},
            "root_pane": {"pane_id": "pane-1", "terminal_id": terminal_id}
        }
    })
    .to_string()
    .into_bytes()
}

fn pane_info(terminal_id: Option<&str>) -> Vec<u8> {
    serde_json::json!({
        "result": {
            "pane": {"pane_id": "pane-1", "terminal_id": terminal_id}
        }
    })
    .to_string()
    .into_bytes()
}

fn prompted() -> Vec<u8> {
    br#"{"result":{"prompted":true}}"#.to_vec()
}

fn launch<'a>(args: &'a [String], cwd: &'a Path, prompt: &'a str) -> Launch<'a> {
    Launch {
        args,
        cwd,
        kind: AgentKind::Claude,
        label: "Notes",
        prompt,
        workspace_id: "workspace-1",
    }
}

fn run_launch(
    input: &Launch<'_>,
    outputs: Vec<sheprd_client::Result<Vec<u8>>>,
) -> (Result<Launched>, Vec<Vec<String>>, Vec<Step>) {
    let mut outputs = VecDeque::from(outputs);
    let mut calls: Vec<Vec<String>> = Vec::new();
    let mut reports = Vec::new();
    let cancelled = AtomicBool::new(false);
    let result = launch_core(
        input,
        || "doc-agent-1".to_owned(),
        |step| reports.push(step),
        &mut |args, _| {
            calls.push(args.iter().map(|arg| (*arg).to_owned()).collect());
            outputs.pop_front().expect("test output")
        },
        |_| {},
        || Duration::ZERO,
        &cancelled,
    );
    (result, calls, reports)
}

#[test]
fn launch_uses_exact_argv_and_reports_steps() {
    let args = vec!["--model".to_owned(), "fast".to_owned()];
    let cwd = Path::new("/tmp/notes");
    let input = launch(&args, cwd, "start prompt");
    let (result, calls, reports) = run_launch(
        &input,
        vec![
            Ok(tab_created(Some("terminal-1"))),
            Ok(br#"{"result":{"started":true}}"#.to_vec()),
            Ok(Vec::new()),
            Ok(prompted()),
        ],
    );

    assert_eq!(
        result.unwrap(),
        Launched {
            agent_name: "doc-agent-1".to_owned(),
            pane_id: "pane-1".to_owned(),
            tab_id: "tab-1".to_owned(),
        }
    );
    assert_eq!(
        calls,
        vec![
            vec![
                "tab",
                "create",
                "--workspace",
                "workspace-1",
                "--cwd",
                "/tmp/notes",
                "--label",
                "Notes",
                "--no-focus"
            ],
            vec![
                "agent",
                "start",
                "doc-agent-1",
                "--kind",
                "claude",
                "--pane",
                "pane-1",
                "--timeout",
                "45000",
                "--",
                "--model",
                "fast"
            ],
            vec!["pane", "rename", "pane-1", "Notes"],
            vec!["agent", "prompt", "doc-agent-1", "start prompt"],
        ]
        .into_iter()
        .map(|call| call.into_iter().map(str::to_owned).collect())
        .collect::<Vec<Vec<String>>>()
    );
    assert_eq!(
        reports,
        vec![
            Step::TabCreated {
                pane_id: "pane-1".to_owned(),
                tab_id: "tab-1".to_owned(),
            },
            Step::AgentStarted(Launched {
                agent_name: "doc-agent-1".to_owned(),
                pane_id: "pane-1".to_owned(),
                tab_id: "tab-1".to_owned(),
            }),
        ]
    );
}

#[test]
fn launch_omits_argument_separator_when_there_are_no_arguments() {
    let args = Vec::new();
    let cwd = Path::new("/tmp/notes");
    let input = launch(&args, cwd, "start prompt");
    let (_, calls, _) = run_launch(
        &input,
        vec![
            Ok(tab_created(Some("terminal-1"))),
            Ok(br#"{"result":{"started":true}}"#.to_vec()),
            Ok(Vec::new()),
            Ok(prompted()),
        ],
    );

    assert_eq!(
        calls[1],
        vec![
            "agent",
            "start",
            "doc-agent-1",
            "--kind",
            "claude",
            "--pane",
            "pane-1",
            "--timeout",
            "45000"
        ]
        .into_iter()
        .map(str::to_owned)
        .collect::<Vec<_>>()
    );
}

#[cfg(unix)]
fn rejection(id: &str, code: &str) -> sheprd_client::Error {
    use std::os::unix::process::ExitStatusExt;

    sheprd_client::Error::ScriptExit {
        status: std::process::ExitStatus::from_raw(256),
        stderr: serde_json::json!({"id": id, "error": {"code": code}}).to_string(),
    }
}

#[cfg(unix)]
#[test]
fn launch_tries_the_next_name_when_the_first_is_taken() {
    let args = Vec::new();
    let cwd = Path::new("/tmp/notes");
    let input = launch(&args, cwd, "start prompt");
    let mut names = VecDeque::from(["first", "second"]);
    let mut outputs = VecDeque::from([
        Ok(tab_created(Some("terminal-1"))),
        Err(rejection("cli:agent:start", "agent_name_taken")),
        Ok(br#"{"result":{"started":true}}"#.to_vec()),
        Ok(Vec::new()),
        Ok(prompted()),
    ]);
    let mut calls: Vec<Vec<String>> = Vec::new();
    let cancelled = AtomicBool::new(false);
    let result = launch_core(
        &input,
        || names.pop_front().expect("test name").to_owned(),
        |_| {},
        &mut |args, _| {
            calls.push(args.iter().map(|arg| (*arg).to_owned()).collect());
            outputs.pop_front().expect("test output")
        },
        |_| {},
        || Duration::ZERO,
        &cancelled,
    );

    assert_eq!(result.unwrap().agent_name, "second");
    assert_eq!(calls[1][2], "first");
    assert_eq!(calls[2][2], "second");
}

#[cfg(unix)]
#[test]
fn three_taken_names_close_the_created_tab() {
    let args = Vec::new();
    let cwd = Path::new("/tmp/notes");
    let input = launch(&args, cwd, "start prompt");
    let mut names = VecDeque::from(["first", "second", "third"]);
    let mut outputs = VecDeque::from([
        Ok(tab_created(Some("terminal-1"))),
        Err(rejection("cli:agent:start", "agent_name_taken")),
        Err(rejection("cli:agent:start", "agent_name_taken")),
        Err(rejection("cli:agent:start", "agent_name_taken")),
        Ok(Vec::new()),
    ]);
    let mut calls: Vec<Vec<String>> = Vec::new();
    let cancelled = AtomicBool::new(false);
    let result = launch_core(
        &input,
        || names.pop_front().expect("test name").to_owned(),
        |_| {},
        &mut |args, _| {
            calls.push(args.iter().map(|arg| (*arg).to_owned()).collect());
            outputs.pop_front().expect("test output")
        },
        |_| {},
        || Duration::ZERO,
        &cancelled,
    );

    assert!(matches!(result, Err(Error::NameTaken { .. })));
    assert_eq!(calls.last().unwrap(), &["tab", "close", "tab-1"]);
    assert_eq!(calls.iter().filter(|call| call[0] == "agent").count(), 3);
}

#[cfg(unix)]
#[test]
fn busy_retries_after_the_same_terminal_is_reported() {
    let args = Vec::new();
    let cwd = Path::new("/tmp/notes");
    let input = launch(&args, cwd, "start prompt");
    let mut outputs = VecDeque::from([
        Ok(tab_created(Some("terminal-1"))),
        Err(rejection("cli:agent:start", "agent_pane_busy")),
        Ok(pane_info(Some("terminal-1"))),
        Ok(br#"{"result":{"started":true}}"#.to_vec()),
        Ok(Vec::new()),
        Ok(prompted()),
    ]);
    let mut calls: Vec<Vec<String>> = Vec::new();
    let clock = Cell::new(Duration::ZERO);
    let cancelled = AtomicBool::new(false);
    let result = launch_core(
        &input,
        || "doc-agent-1".to_owned(),
        |_| {},
        &mut |args, _| {
            calls.push(args.iter().map(|arg| (*arg).to_owned()).collect());
            outputs.pop_front().expect("test output")
        },
        |pause| clock.set(clock.get() + pause),
        || clock.get(),
        &cancelled,
    );

    assert!(result.is_ok());
    assert_eq!(calls[1][0], "agent");
    assert_eq!(calls[2], vec!["pane", "get", "pane-1"]);
    assert_eq!(calls[3][0], "agent");
    assert_eq!(clock.get(), Duration::from_millis(100));
}

#[cfg(unix)]
#[test]
fn busy_with_a_changed_terminal_is_not_retried() {
    let args = Vec::new();
    let cwd = Path::new("/tmp/notes");
    let input = launch(&args, cwd, "start prompt");
    let mut outputs = VecDeque::from([
        Ok(tab_created(Some("terminal-1"))),
        Err(rejection("cli:agent:start", "agent_pane_busy")),
        Ok(pane_info(Some("terminal-2"))),
        Ok(Vec::new()),
    ]);
    let mut calls: Vec<Vec<String>> = Vec::new();
    let cancelled = AtomicBool::new(false);
    let result = launch_core(
        &input,
        || "doc-agent-1".to_owned(),
        |_| {},
        &mut |args, _| {
            calls.push(args.iter().map(|arg| (*arg).to_owned()).collect());
            outputs.pop_front().expect("test output")
        },
        |_| {},
        || Duration::ZERO,
        &cancelled,
    );

    assert!(matches!(
        result,
        Err(Error::Script {
            step: JobStep::StartAgent,
            ..
        })
    ));
    assert_eq!(calls.len(), 4);
    assert_eq!(calls.last().unwrap(), &["tab", "close", "tab-1"]);
}

#[cfg(unix)]
#[test]
fn busy_without_terminal_identity_is_not_retried() {
    let args = Vec::new();
    let cwd = Path::new("/tmp/notes");
    let input = launch(&args, cwd, "start prompt");
    let mut outputs = VecDeque::from([
        Ok(tab_created(None)),
        Err(rejection("cli:agent:start", "agent_pane_busy")),
        Ok(Vec::new()),
    ]);
    let mut calls: Vec<Vec<String>> = Vec::new();
    let cancelled = AtomicBool::new(false);
    let result = launch_core(
        &input,
        || "doc-agent-1".to_owned(),
        |_| {},
        &mut |args, _| {
            calls.push(args.iter().map(|arg| (*arg).to_owned()).collect());
            outputs.pop_front().expect("test output")
        },
        |_| {},
        || Duration::ZERO,
        &cancelled,
    );

    assert!(matches!(
        result,
        Err(Error::Script {
            step: JobStep::StartAgent,
            ..
        })
    ));
    assert_eq!(calls.len(), 3);
    assert_eq!(calls.last().unwrap(), &["tab", "close", "tab-1"]);
}

#[test]
fn invalid_agent_names_are_rejected_before_start() {
    let args = Vec::new();
    let cwd = Path::new("/tmp/notes");
    let input = launch(&args, cwd, "start prompt");
    let mut outputs = VecDeque::from([Ok(tab_created(Some("terminal-1"))), Ok(Vec::new())]);
    let mut calls: Vec<Vec<String>> = Vec::new();
    let cancelled = AtomicBool::new(false);
    let result = launch_core(
        &input,
        || "not.valid".to_owned(),
        |_| {},
        &mut |args, _| {
            calls.push(args.iter().map(|arg| (*arg).to_owned()).collect());
            outputs.pop_front().expect("test output")
        },
        |_| {},
        || Duration::ZERO,
        &cancelled,
    );

    assert!(matches!(result, Err(Error::InvalidAgentName)));
    assert_eq!(calls.len(), 2);
    assert_eq!(calls.last().unwrap(), &["tab", "close", "tab-1"]);
}

#[test]
fn start_timeout_keeps_the_tab_and_is_not_retried() {
    let args = Vec::new();
    let cwd = Path::new("/tmp/notes");
    let input = launch(&args, cwd, "start prompt");
    let mut outputs = VecDeque::from([
        Ok(tab_created(Some("terminal-1"))),
        Err(sheprd_client::Error::ScriptTimeout),
    ]);
    let mut calls: Vec<Vec<String>> = Vec::new();
    let cancelled = AtomicBool::new(false);
    let result = launch_core(
        &input,
        || "doc-agent-1".to_owned(),
        |_| {},
        &mut |args, _| {
            calls.push(args.iter().map(|arg| (*arg).to_owned()).collect());
            outputs.pop_front().expect("test output")
        },
        |_| {},
        || Duration::ZERO,
        &cancelled,
    );

    assert!(matches!(
        result,
        Err(Error::Script {
            step: JobStep::StartAgent,
            source: sheprd_client::Error::ScriptTimeout
        })
    ));
    assert_eq!(calls.len(), 2);
}

#[test]
fn cancelled_before_start_makes_no_calls() {
    let args = Vec::new();
    let cwd = Path::new("/tmp/notes");
    let input = launch(&args, cwd, "start prompt");
    let cancelled = AtomicBool::new(true);
    let mut calls = 0;
    let result = launch_core(
        &input,
        || "doc-agent-1".to_owned(),
        |_| {},
        &mut |_, _| {
            calls += 1;
            Ok(Vec::new())
        },
        |_| {},
        || Duration::ZERO,
        &cancelled,
    );

    assert!(matches!(result, Err(Error::Cancelled)));
    assert_eq!(calls, 0);
}

#[test]
fn cancellation_after_tab_creation_closes_only_that_tab() {
    let args = Vec::new();
    let input = launch(&args, Path::new("/tmp/notes"), "start prompt");
    let cancelled = AtomicBool::new(false);
    let mut outputs = VecDeque::from([
        Ok(tab_created(Some("terminal-1"))),
        Err(sheprd_client::Error::ScriptTimeout),
    ]);
    let mut calls: Vec<Vec<String>> = Vec::new();
    let result = launch_core(
        &input,
        || "doc-agent-1".to_owned(),
        |step| {
            if matches!(step, Step::TabCreated { .. }) {
                cancelled.store(true, std::sync::atomic::Ordering::Release);
            }
        },
        &mut |args, _| {
            calls.push(args.iter().map(|arg| (*arg).to_owned()).collect());
            outputs.pop_front().expect("test output")
        },
        |_| {},
        || Duration::ZERO,
        &cancelled,
    );
    assert!(matches!(result, Err(Error::Cancelled)));
    assert_eq!(calls.len(), 2);
    assert_eq!(calls[1], ["tab", "close", "tab-1"]);
}

#[test]
fn cancellation_after_agent_started_preserves_the_tab() {
    let args = Vec::new();
    let input = launch(&args, Path::new("/tmp/notes"), "start prompt");
    let cancelled = AtomicBool::new(false);
    let mut outputs = VecDeque::from([
        Ok(tab_created(Some("terminal-1"))),
        Ok(br#"{"result":{"started":true}}"#.to_vec()),
    ]);
    let mut calls: Vec<Vec<String>> = Vec::new();
    let result = launch_core(
        &input,
        || "doc-agent-1".to_owned(),
        |step| {
            if matches!(step, Step::AgentStarted(_)) {
                cancelled.store(true, std::sync::atomic::Ordering::Release);
            }
        },
        &mut |args, _| {
            calls.push(args.iter().map(|arg| (*arg).to_owned()).collect());
            outputs.pop_front().expect("test output")
        },
        |_| {},
        || Duration::ZERO,
        &cancelled,
    );
    assert!(matches!(result, Err(Error::Cancelled)));
    assert_eq!(calls.len(), 2);
    assert!(
        !calls
            .iter()
            .any(|call| call[0] == "tab" && call[1] == "close")
    );
}

#[test]
fn an_agent_command_that_could_not_spawn_closes_the_tab() {
    let args = Vec::new();
    let input = launch(&args, Path::new("/tmp/notes"), "start prompt");
    let (result, calls, _) = run_launch(
        &input,
        vec![
            Ok(tab_created(Some("terminal-1"))),
            Err(sheprd_client::Error::ScriptSpawn(std::io::Error::other(
                "cannot spawn",
            ))),
            Ok(Vec::new()),
        ],
    );
    assert!(matches!(
        result,
        Err(Error::Script {
            step: JobStep::StartAgent,
            ..
        })
    ));
    assert_eq!(calls.last().unwrap(), &["tab", "close", "tab-1"]);
}

#[test]
fn an_uncertain_start_transport_error_preserves_the_tab() {
    let args = Vec::new();
    let input = launch(&args, Path::new("/tmp/notes"), "start prompt");
    let (result, calls, _) = run_launch(
        &input,
        vec![
            Ok(tab_created(Some("terminal-1"))),
            Err(sheprd_client::Error::ScriptIo(std::io::Error::other(
                "lost response",
            ))),
        ],
    );
    assert!(matches!(
        result,
        Err(Error::Script {
            step: JobStep::StartAgent,
            ..
        })
    ));
    assert_eq!(calls.len(), 2);
}

#[cfg(unix)]
#[test]
fn cli_start_timeouts_and_uncertain_failures_preserve_the_tab() {
    let args = Vec::new();
    let input = launch(&args, Path::new("/tmp/notes"), "start prompt");
    for code in [
        "timeout",
        "agent_start_transport_failed",
        "agent_not_ready",
        "agent_start_failed",
    ] {
        let (result, calls, _) = run_launch(
            &input,
            vec![
                Ok(tab_created(Some("terminal-1"))),
                Err(rejection("cli:agent:start", code)),
            ],
        );
        assert!(matches!(
            result,
            Err(Error::Script {
                step: JobStep::StartAgent,
                ..
            })
        ));
        assert_eq!(calls.len(), 2);
    }
}

#[test]
fn prompt_success_is_sent() {
    let cancelled = AtomicBool::new(false);
    let mut calls: Vec<Vec<String>> = Vec::new();
    let result = prompt_core(
        "agent-1",
        "change prompt",
        &mut |args, _| {
            calls.push(args.iter().map(|arg| (*arg).to_owned()).collect());
            Ok(prompted())
        },
        &cancelled,
    );

    assert!(matches!(result, Ok(PromptOutcome::Sent)));
    assert_eq!(
        calls,
        vec![vec!["agent", "prompt", "agent-1", "change prompt"]]
    );
}

#[cfg(unix)]
#[test]
fn launch_retries_a_not_ready_start_prompt_until_sent() {
    let args = Vec::new();
    let cwd = Path::new("/tmp/notes");
    let input = launch(&args, cwd, "start prompt");
    let mut outputs = VecDeque::from([
        Ok(tab_created(Some("terminal-1"))),
        Ok(br#"{"result":{"started":true}}"#.to_vec()),
        Ok(Vec::new()),
        Err(rejection("cli:agent:prompt", "agent_not_ready")),
        Err(rejection("cli:agent:prompt", "agent_launch_pending")),
        Ok(prompted()),
    ]);
    let mut calls: Vec<Vec<String>> = Vec::new();
    let clock = Cell::new(Duration::ZERO);
    let cancelled = AtomicBool::new(false);
    let result = launch_core(
        &input,
        || "doc-agent-1".to_owned(),
        |_| {},
        &mut |args, _| {
            calls.push(args.iter().map(|arg| (*arg).to_owned()).collect());
            outputs.pop_front().expect("test output")
        },
        |pause| clock.set(clock.get() + pause),
        || clock.get(),
        &cancelled,
    );

    assert!(result.is_ok());
    assert_eq!(calls.len(), 6);
    assert_eq!(
        calls[3],
        vec!["agent", "prompt", "doc-agent-1", "start prompt"]
    );
    assert_eq!(clock.get(), Duration::from_secs(1));
}

#[cfg(unix)]
#[test]
fn launch_stops_not_ready_prompt_retries_at_the_window() {
    let args = Vec::new();
    let cwd = Path::new("/tmp/notes");
    let input = launch(&args, cwd, "start prompt");
    let mut outputs = VecDeque::from([
        Ok(tab_created(Some("terminal-1"))),
        Ok(br#"{"result":{"started":true}}"#.to_vec()),
        Ok(Vec::new()),
        Err(rejection("cli:agent:prompt", "agent_not_ready")),
    ]);
    let mut calls: Vec<Vec<String>> = Vec::new();
    let clock = Cell::new(Duration::ZERO);
    let cancelled = AtomicBool::new(false);
    let result = launch_core(
        &input,
        || "doc-agent-1".to_owned(),
        |_| {},
        &mut |args, _| {
            calls.push(args.iter().map(|arg| (*arg).to_owned()).collect());
            outputs.pop_front().expect("test output")
        },
        |_| clock.set(Duration::from_secs(30)),
        || clock.get(),
        &cancelled,
    );

    assert!(matches!(result, Err(Error::NotReady { .. })));
    assert_eq!(calls.len(), 4);
}

#[cfg(unix)]
#[test]
fn launch_does_not_retry_a_gone_start_prompt() {
    let args = Vec::new();
    let cwd = Path::new("/tmp/notes");
    let input = launch(&args, cwd, "start prompt");
    let mut outputs = VecDeque::from([
        Ok(tab_created(Some("terminal-1"))),
        Ok(br#"{"result":{"started":true}}"#.to_vec()),
        Ok(Vec::new()),
        Err(rejection("cli:agent:prompt", "agent_not_found")),
    ]);
    let mut calls: Vec<Vec<String>> = Vec::new();
    let cancelled = AtomicBool::new(false);
    let result = launch_core(
        &input,
        || "doc-agent-1".to_owned(),
        |_| {},
        &mut |args, _| {
            calls.push(args.iter().map(|arg| (*arg).to_owned()).collect());
            outputs.pop_front().expect("test output")
        },
        |_| {},
        || Duration::ZERO,
        &cancelled,
    );

    assert!(matches!(result, Err(Error::AgentGone { .. })));
    assert_eq!(calls.len(), 4);
}

#[cfg(unix)]
#[test]
fn prompt_refusal_codes_are_classified() {
    for (code, expected) in [
        ("agent_blocked", PromptRefusal::NotReady),
        ("agent_launch_pending", PromptRefusal::NotReady),
        ("agent_not_ready", PromptRefusal::NotReady),
        ("agent_not_found", PromptRefusal::Gone),
        ("agent_not_running", PromptRefusal::Gone),
    ] {
        let mut error = Some(rejection("cli:agent:prompt", code));
        let cancelled = AtomicBool::new(false);
        let result = prompt_core(
            "agent-1",
            "prompt",
            &mut |_, _| Err(error.take().expect("prompt error")),
            &cancelled,
        );
        assert!(matches!(
            result,
            Ok(PromptOutcome::Refused { refusal, .. }) if refusal == expected
        ));
    }
}

#[cfg(unix)]
#[test]
fn stalled_prompt_is_an_error() {
    let mut error = Some(rejection("cli:agent:prompt", "agent_prompt_stalled"));
    let cancelled = AtomicBool::new(false);
    let result = prompt_core(
        "agent-1",
        "prompt",
        &mut |_, _| Err(error.take().expect("prompt error")),
        &cancelled,
    );

    assert!(matches!(
        result,
        Err(Error::Script {
            step: JobStep::Prompt,
            ..
        })
    ));
}

#[test]
fn malformed_tab_output_is_an_output_error() {
    let args = Vec::new();
    let cwd = Path::new("/tmp/notes");
    let input = launch(&args, cwd, "start prompt");
    let (result, _, _) = run_launch(&input, vec![Ok(b"{".to_vec())]);

    assert!(matches!(
        result,
        Err(Error::Output {
            step: JobStep::CreateTab,
            ..
        })
    ));
}
