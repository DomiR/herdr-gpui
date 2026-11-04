//! Blocking host calls used to create and prompt a document agent.

use super::{Error, JobStep, Result, valid_agent_name};
use crate::teleport::{
    AgentKind, Envelope, Host,
    snapshot::{PaneInfoResult, TabCreated},
};
use serde::Deserialize;
use std::{
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

const AGENT_START_IDLE: Duration = Duration::from_secs(60);
const AGENT_START_TIMEOUT: &str = "45000";
const NAME_ATTEMPTS: usize = 3;
const POLL_INTERVAL: Duration = Duration::from_millis(100);
const PROMPT_READY_TIMEOUT: Duration = Duration::from_secs(30);
const PROMPT_RETRY_INTERVAL: Duration = Duration::from_millis(500);
const SHELL_START_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug)]
pub(crate) struct Launch<'a> {
    pub(crate) args: &'a [String],
    pub(crate) cwd: &'a Path,
    pub(crate) kind: AgentKind,
    pub(crate) label: &'a str,
    pub(crate) prompt: &'a str,
    pub(crate) workspace_id: &'a str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Launched {
    pub(crate) agent_name: String,
    pub(crate) pane_id: String,
    pub(crate) tab_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Step {
    TabCreated { pane_id: String, tab_id: String },
    AgentStarted(Launched),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Delivery {
    Sent,
    NotReady,
    Gone,
}

pub(crate) fn launch(
    host: &Host,
    launch: &Launch<'_>,
    names: impl FnMut() -> String,
    report: impl FnMut(Step),
    cancelled: &AtomicBool,
) -> Result<Launched> {
    let started = Instant::now();
    let cleanup = AtomicBool::new(false);
    let mut exec = |args: &[&str], idle| {
        // Closing our unstarted tab must still work after cancellation.
        let cancel = if args.starts_with(&["tab", "close"]) {
            &cleanup
        } else {
            cancelled
        };
        host.herdr_exec(args, idle, cancel)
    };
    launch_core(
        launch,
        names,
        report,
        &mut exec,
        std::thread::sleep,
        || started.elapsed(),
        cancelled,
    )
}

pub(crate) fn prompt(
    host: &Host,
    agent_name: &str,
    text: &str,
    cancelled: &AtomicBool,
) -> Result<Delivery> {
    let mut exec = |args: &[&str], idle| host.herdr_exec(args, idle, cancelled);
    prompt_core(agent_name, text, &mut exec, cancelled).map(|outcome| match outcome {
        PromptOutcome::Sent => Delivery::Sent,
        PromptOutcome::Refused {
            refusal: PromptRefusal::NotReady,
            ..
        } => Delivery::NotReady,
        PromptOutcome::Refused {
            refusal: PromptRefusal::Gone,
            ..
        } => Delivery::Gone,
    })
}

pub(super) fn launch_core(
    launch: &Launch<'_>,
    mut names: impl FnMut() -> String,
    mut report: impl FnMut(Step),
    exec: &mut impl FnMut(&[&str], Duration) -> herdr_client::Result<Vec<u8>>,
    mut pause: impl FnMut(Duration),
    mut elapsed: impl FnMut() -> Duration,
    cancelled: &AtomicBool,
) -> Result<Launched> {
    check_cancelled(cancelled)?;
    let cwd = launch.cwd.to_string_lossy().into_owned();
    let create_args = [
        "tab",
        "create",
        "--workspace",
        launch.workspace_id,
        "--cwd",
        cwd.as_str(),
        "--label",
        launch.label,
        "--no-focus",
    ];
    let output = exec(&create_args, Host::QUERY_IDLE).map_err(|source| Error::Script {
        step: JobStep::CreateTab,
        source,
    })?;
    let created: Envelope<TabCreated> =
        serde_json::from_slice(&output).map_err(|source| Error::Output {
            step: JobStep::CreateTab,
            source,
        })?;
    let tab_id = created.result.tab.tab_id;
    let pane_id = created.result.root_pane.pane_id;
    let terminal_id = created.result.root_pane.terminal_id;
    let mut may_be_running = false;
    let result = {
        let mut exec = |args: &[&str], idle| {
            let result = exec(args, idle);
            if args.starts_with(&["agent", "start"]) && !result.as_ref().is_err_and(start_refused) {
                may_be_running = true;
            }
            result
        };
        (|| {
            check_cancelled(cancelled)?;
            report(Step::TabCreated {
                pane_id: pane_id.clone(),
                tab_id: tab_id.clone(),
            });
            let mut attempt = 0;
            let launched = loop {
                check_cancelled(cancelled)?;
                let agent_name = names();
                if !valid_agent_name(&agent_name) {
                    return Err(Error::InvalidAgentName);
                }
                let mut start_args = vec![
                    "agent",
                    "start",
                    agent_name.as_str(),
                    "--kind",
                    launch.kind.name(),
                    "--pane",
                    pane_id.as_str(),
                    "--timeout",
                    AGENT_START_TIMEOUT,
                ];
                if !launch.args.is_empty() {
                    start_args.push("--");
                    start_args.extend(launch.args.iter().map(String::as_str));
                }
                let started = match exec(&start_args, AGENT_START_IDLE) {
                    Ok(_) => Ok(()),
                    Err(source) if is_name_taken(&source) => Err(Error::NameTaken { source }),
                    Err(source) if is_busy(&source) => {
                        let Some(terminal_id) = terminal_id.as_deref().filter(|id| !id.is_empty())
                        else {
                            return Err(Error::Script {
                                step: JobStep::StartAgent,
                                source,
                            });
                        };
                        retry_busy(
                            &mut exec,
                            &start_args,
                            &pane_id,
                            terminal_id,
                            source,
                            &mut pause,
                            &mut elapsed,
                            cancelled,
                        )
                    }
                    Err(source) => Err(Error::Script {
                        step: JobStep::StartAgent,
                        source,
                    }),
                };
                match started {
                    Ok(()) => {
                        let launched = Launched {
                            agent_name,
                            pane_id: pane_id.clone(),
                            tab_id: tab_id.clone(),
                        };
                        report(Step::AgentStarted(launched.clone()));
                        break launched;
                    }
                    Err(Error::NameTaken { .. }) if attempt + 1 < NAME_ATTEMPTS => attempt += 1,
                    Err(error) => return Err(error),
                }
            };
            check_cancelled(cancelled)?;
            let _ = exec(
                &["pane", "rename", launched.pane_id.as_str(), launch.label],
                Host::QUERY_IDLE,
            );
            launch_prompt(
                &launched.agent_name,
                launch.prompt,
                &mut exec,
                &mut pause,
                &mut elapsed,
                cancelled,
            )?;
            Ok(launched)
        })()
    };
    if result.is_err() && !may_be_running {
        let _ = exec(&["tab", "close", &tab_id], Host::QUERY_IDLE);
    }
    result
}

fn launch_prompt(
    agent_name: &str,
    text: &str,
    exec: &mut impl FnMut(&[&str], Duration) -> herdr_client::Result<Vec<u8>>,
    pause: &mut impl FnMut(Duration),
    elapsed: &mut impl FnMut() -> Duration,
    cancelled: &AtomicBool,
) -> Result<()> {
    let retry_started = elapsed();
    loop {
        check_cancelled(cancelled)?;
        match prompt_core(agent_name, text, exec, cancelled)? {
            PromptOutcome::Sent => return Ok(()),
            PromptOutcome::Refused {
                refusal: PromptRefusal::Gone,
                source,
            } => return Err(Error::AgentGone { source }),
            PromptOutcome::Refused {
                refusal: PromptRefusal::NotReady,
                source,
            } => {
                let remaining =
                    PROMPT_READY_TIMEOUT.saturating_sub(elapsed().saturating_sub(retry_started));
                if remaining.is_zero() {
                    return Err(Error::NotReady { source });
                }
                pause(PROMPT_RETRY_INTERVAL.min(remaining));
                if elapsed().saturating_sub(retry_started) >= PROMPT_READY_TIMEOUT {
                    return Err(Error::NotReady { source });
                }
            }
        }
    }
}

pub(super) fn prompt_core(
    agent_name: &str,
    text: &str,
    exec: &mut impl FnMut(&[&str], Duration) -> herdr_client::Result<Vec<u8>>,
    cancelled: &AtomicBool,
) -> Result<PromptOutcome> {
    check_cancelled(cancelled)?;
    let output = match exec(&["agent", "prompt", agent_name, text], Host::QUERY_IDLE) {
        Ok(output) => output,
        Err(source) => {
            let Some(refusal) = rejection_refusal(&source) else {
                return Err(Error::Script {
                    step: JobStep::Prompt,
                    source,
                });
            };
            return Ok(PromptOutcome::Refused { refusal, source });
        }
    };
    let _: Envelope<serde_json::Value> =
        serde_json::from_slice(&output).map_err(|source| Error::Output {
            step: JobStep::Prompt,
            source,
        })?;
    Ok(PromptOutcome::Sent)
}

pub(super) enum PromptOutcome {
    Sent,
    Refused {
        refusal: PromptRefusal,
        source: herdr_client::Error,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PromptRefusal {
    Gone,
    NotReady,
}

fn retry_busy(
    exec: &mut impl FnMut(&[&str], Duration) -> herdr_client::Result<Vec<u8>>,
    start_args: &[&str],
    pane_id: &str,
    terminal_id: &str,
    mut error: herdr_client::Error,
    pause: &mut impl FnMut(Duration),
    elapsed: &mut impl FnMut() -> Duration,
    cancelled: &AtomicBool,
) -> Result<()> {
    loop {
        check_cancelled(cancelled)?;
        let remaining = SHELL_START_TIMEOUT.saturating_sub(elapsed());
        if remaining.is_zero() {
            return Err(Error::Script {
                step: JobStep::StartAgent,
                source: error,
            });
        }
        pause(POLL_INTERVAL.min(remaining));
        check_cancelled(cancelled)?;
        let remaining = SHELL_START_TIMEOUT.saturating_sub(elapsed());
        if remaining.is_zero() {
            return Err(Error::Script {
                step: JobStep::StartAgent,
                source: error,
            });
        }
        let pane_args = ["pane", "get", pane_id];
        let output = exec(&pane_args, remaining).map_err(|source| Error::Script {
            step: JobStep::PaneQuery,
            source,
        })?;
        let current: Envelope<PaneInfoResult> =
            serde_json::from_slice(&output).map_err(|source| Error::Output {
                step: JobStep::PaneQuery,
                source,
            })?;
        if current.result.pane.pane_id != pane_id
            || current.result.pane.terminal_id.as_deref() != Some(terminal_id)
        {
            // Match actions::start::retry_busy: a missing or changed terminal never authorizes a retry.
            return Err(Error::Script {
                step: JobStep::StartAgent,
                source: error,
            });
        }
        check_cancelled(cancelled)?;
        match exec(start_args, AGENT_START_IDLE) {
            Ok(_) => return Ok(()),
            Err(source) if is_name_taken(&source) => return Err(Error::NameTaken { source }),
            Err(source) if is_busy(&source) => error = source,
            Err(source) => {
                return Err(Error::Script {
                    step: JobStep::StartAgent,
                    source,
                });
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum RejectionCode {
    AgentBlocked,
    AgentLaunchPending,
    AgentNameTaken,
    AgentNotFound,
    AgentNotReady,
    AgentNotRunning,
    AgentPaneBusy,
    AgentPaneNotFound,
    AgentStartInputFailed,
    InvalidAgentArgument,
    InvalidAgentName,
    InvalidAgentTimeout,
    UnsupportedAgentKind,
    #[serde(other)]
    Other,
}

#[derive(Deserialize)]
struct Rejection {
    id: String,
    error: RejectionBody,
}

#[derive(Deserialize)]
struct RejectionBody {
    code: RejectionCode,
}

fn rejection(error: &herdr_client::Error) -> Option<Rejection> {
    let herdr_client::Error::ScriptExit { status, stderr } = error else {
        return None;
    };
    if status.code() != Some(1) {
        return None;
    }
    serde_json::from_str::<Rejection>(stderr).ok()
}

fn is_name_taken(error: &herdr_client::Error) -> bool {
    rejection(error).is_some_and(|rejection| {
        rejection.id == "cli:agent:start" && rejection.error.code == RejectionCode::AgentNameTaken
    })
}

fn is_busy(error: &herdr_client::Error) -> bool {
    rejection(error).is_some_and(|rejection| {
        rejection.id == "cli:agent:start" && rejection.error.code == RejectionCode::AgentPaneBusy
    })
}

fn start_refused(error: &herdr_client::Error) -> bool {
    if matches!(
        error,
        herdr_client::Error::ScriptSpawn(_) | herdr_client::Error::ScriptUnsupported
    ) {
        return true;
    }
    // Only rejections before launch prove that cleanup cannot kill an agent.
    // Unknown CLI errors include startup timeouts and transport failures.
    rejection(error).is_some_and(|rejection| {
        rejection.id == "cli:agent:start"
            && matches!(
                rejection.error.code,
                RejectionCode::AgentNameTaken
                    | RejectionCode::AgentPaneBusy
                    | RejectionCode::AgentPaneNotFound
                    | RejectionCode::AgentStartInputFailed
                    | RejectionCode::InvalidAgentArgument
                    | RejectionCode::InvalidAgentName
                    | RejectionCode::InvalidAgentTimeout
                    | RejectionCode::UnsupportedAgentKind
            )
    })
}

fn rejection_refusal(error: &herdr_client::Error) -> Option<PromptRefusal> {
    let rejection = rejection(error)?;
    if rejection.id != "cli:agent:prompt" {
        return None;
    }
    match rejection.error.code {
        RejectionCode::AgentBlocked
        | RejectionCode::AgentLaunchPending
        | RejectionCode::AgentNotReady => Some(PromptRefusal::NotReady),
        RejectionCode::AgentNotFound | RejectionCode::AgentNotRunning => Some(PromptRefusal::Gone),
        _ => None,
    }
}

fn check_cancelled(cancelled: &AtomicBool) -> Result<()> {
    if cancelled.load(Ordering::Acquire) {
        Err(Error::Cancelled)
    } else {
        Ok(())
    }
}
