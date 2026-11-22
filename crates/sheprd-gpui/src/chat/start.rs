//! Starting a new chat from this window: a tab in the focused workspace, in
//! the focused pane's folder, running the `herdr-acp` that came with this
//! app for the agent the user picked. Once the program reports its pane,
//! discovery attaches the chat view as for any other chat.
//!
//! The endpoint API has no method that starts a command, so this follows the
//! closed-agent reopen: `tab.create` through the connection's script slot,
//! then one fixed line typed into the shell of the pane it made. The
//! program's path rides in the tab's environment rather than in the line,
//! so nothing typed depends on where the app is installed, and the agent's
//! name comes from the shared table, never from daemon text.

use crate::{
    NavigationTarget, SheprdWindow,
    browser::GroupId,
    i18n::{Localize, fl},
    window::Flash,
};
use gpui::{prelude::*, *};
use serde_json::json;
use sheprd_chat_protocol::{Agent, Settings};
use sheprd_client::Method;
use std::{
    borrow::Cow,
    collections::BTreeMap,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

/// The environment variable the typed line runs the program from.
pub(crate) const PROGRAM_ENV: &str = "HERDR_ACP";
/// The program's file name on this platform.
const PROGRAM: &str = if cfg!(windows) {
    "herdr-acp.exe"
} else {
    "herdr-acp"
};
pub(crate) use sheprd_client::agent_choice::AgentChoice;

/// Whether `program` takes a brief, from its `--help`: an older one would
/// refuse [`super::brief::FILE_OPTION`] and start nothing. Blocking: run it
/// off the UI thread.
pub(crate) fn takes_briefs(program: &Path) -> bool {
    sheprd_client::agent_choice::offers(program, super::brief::FILE_OPTION)
}

/// How long a start may wait for the daemon to make its tab.
const DEADLINE: Duration = Duration::from_secs(15);

#[derive(Debug, thiserror::Error)]
pub enum Error {
    // keep-sorted start
    #[error("{} is not installed on this host", .0.display_name())]
    AgentMissing(Agent),
    #[error("a brief can only start a chat on this machine")]
    BriefRemote,
    #[error("ACP chats are off in Settings")]
    Disabled,
    #[error("the chat's command cannot be typed into a shell")]
    Line,
    #[error("Sheprd is not connected")]
    NotConnected,
    #[error("this pane has no supported chat route")]
    NotLocal,
    #[error("no workspace is focused")]
    NoWorkspace,
    #[error("sheprd-acp was not found beside Sheprd or on PATH")]
    ProgramMissing,
    #[error("Sheprd is still looking for sheprd-acp")]
    ProgramUnknown,
    #[error("{0}")]
    Request(#[source] std::sync::Arc<crate::Error>),
    #[error("Herdr's answer did not name the chat's tab")]
    Response,
    #[error("another tab is still opening")]
    SlotBusy,
    #[error("Herdr did not open the chat's tab in time")]
    Timeout,
    // keep-sorted end
}

/// Where the `herdr-acp` this window starts chats with is, once looked for.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) enum Program {
    #[default]
    Unknown,
    Looking,
    Found(PathBuf),
    Missing,
}

impl Program {
    /// The program found on this machine. A lookup not yet finished is not
    /// a missing program: it may still find one.
    pub(crate) fn found(&self) -> Result<&Path, Error> {
        match self {
            Self::Found(program) => Ok(program),
            Self::Missing => Err(Error::ProgramMissing),
            Self::Unknown | Self::Looking => Err(Error::ProgramUnknown),
        }
    }
}

/// The program beside this app's executable, as the bundle and `cargo
/// build` lay it out, else the first on the login shell's `PATH`. Blocking:
/// run it off the UI thread.
pub(crate) fn locate() -> Option<PathBuf> {
    crate::bundled::locate(PROGRAM)
}

#[cfg(test)]
pub(crate) fn locate_in(exe: Option<&Path>, path: Option<&std::ffi::OsStr>) -> Option<PathBuf> {
    crate::bundled::locate_in(PROGRAM, exe, path)
}

/// The line typed into the new tab's shell. On Unix `sh` runs it, so it
/// reads the same from bash, zsh, fish, or nushell; on Windows it is the
/// PowerShell Herdr gives panes. A leading space keeps it out of the history
/// of shells that honor that. The shell stays when the chat quits, so an
/// agent that failed to start leaves its message on screen.
/// With `brief`, the line reads its file from the tab's environment.
pub(crate) fn command_line(
    agent: Agent,
    choice: AgentChoice,
    brief: Option<&super::brief::Brief>,
) -> String {
    let mut options = choice
        .option()
        .map_or_else(String::new, |option| format!("{option} "));
    if let Some(brief) = brief {
        options.push_str(&brief.line_words());
    }
    if cfg!(windows) {
        format!("& $env:{PROGRAM_ENV} {options}{agent}")
    } else {
        format!(" sh -c 'exec \"${PROGRAM_ENV}\" {options}{agent}'")
    }
}

/// [`command_line`] for a shell Herdr opened without the program in its
/// environment, such as a new worktree's root pane: its path and the
/// `[chat.env]` variables ride in the line. Read by `sh`, so not for
/// Windows panes; `None` when a word holds a quote or control character.
pub(crate) fn shell_line(
    agent: Agent,
    program: &Path,
    choice: AgentChoice,
    env: &BTreeMap<String, String>,
) -> Option<String> {
    let assignments: Vec<String> = env
        .iter()
        .map(|(name, value)| format!("{name}={value}"))
        .collect();
    let mut words = Vec::with_capacity(assignments.len() + 4);
    if !assignments.is_empty() {
        words.push("env");
        words.extend(assignments.iter().map(String::as_str));
    }
    words.push(program.to_str()?);
    words.extend(choice.option());
    words.push(agent.name());
    sheprd_client::quoted_line(&words)
}

/// The line typed into a new tab on an SSH host: the first `herdr-acp`
/// found there, `~/.herdr/bin` before the rest, else a message saying where
/// to install it, which stays on screen.
pub(crate) fn remote_command_line(agent: Agent) -> String {
    format!(
        " sh -c {}",
        sheprd_client::shell_quote(&super::remote_program_script(&agent.to_string()))
    )
}

/// The `tab.create` a chat runs in, labelled after its agent, with the
/// `[chat.env]` variables in its environment. A local chat also carries
/// this app's program there; a remote one finds its host's own.
pub(crate) fn tab_params(
    workspace: &str,
    cwd: Option<&str>,
    agent: Agent,
    program: Option<&Path>,
    extra: &BTreeMap<String, String>,
) -> serde_json::Value {
    let mut params = json!({
        "workspace_id": workspace,
        "label": agent.display_name(),
        "focus": true,
    });
    let mut env: serde_json::Map<String, serde_json::Value> = extra
        .iter()
        .map(|(name, value)| (name.clone(), json!(value)))
        .collect();
    if let Some(program) = program {
        env.insert(PROGRAM_ENV.to_owned(), json!(program.to_string_lossy()));
    }
    if !env.is_empty() {
        params["env"] = serde_json::Value::Object(env);
    }
    if let Some(cwd) = cwd {
        params["cwd"] = json!(cwd);
    }
    params
}

/// A chat whose tab is being made, fenced by the selection, connection, and
/// daemon boot it was asked of, like a worktree script.
#[derive(Debug)]
pub(crate) struct Starting {
    agent: Agent,
    /// The brief the chat starts with, whose file its line names.
    brief: Option<super::brief::Brief>,
    /// Whether the chat runs on an SSH host, so its line finds the host's
    /// own program.
    remote: bool,
    boot: String,
    deadline: Instant,
    endpoint: (u64, u64),
    endpoint_id: String,
    request: String,
}

impl SheprdWindow {
    /// Looks for the program once, off the UI thread.
    pub(super) fn locate_chat_program(&mut self, cx: &mut Context<Self>) {
        if self.chats.program != Program::Unknown {
            return;
        }
        self.chats.program = Program::Looking;
        let executor = cx.background_executor().clone();
        let found = executor.spawn(async { locate() });
        cx.spawn(async move |this, cx| {
            let found = found.await;
            let _ = this.update(cx, |this, cx| {
                this.chats.program = found.clone().map_or(Program::Missing, Program::Found);
                this.refresh_landing_chats();
                cx.notify();
            });
            // Chats start as soon as the program is found; until its help
            // says it offers the choice, they keep the agent they start with.
            if let Some(program) = found {
                let choice = executor
                    .spawn(async move { AgentChoice::of(&program) })
                    .await;
                let _ = this.update(cx, |this, _| this.chats.program_choice = choice);
            }
        })
        .detach();
    }

    /// Whether this window can start a chat now: the selected Herdr runs on
    /// this machine and the program is here, or it runs on an SSH host,
    /// whose own program the new tab looks for.
    pub(crate) fn can_start_chat(&self) -> bool {
        if !self.config.features.acp_chats {
            return false;
        }
        match self.chat_route() {
            Some(super::Route::Local) => matches!(self.chats.program, Program::Found(_)),
            Some(super::Route::Ssh(_) | super::Route::Remote(_)) => true,
            None => false,
        }
    }

    /// [`Self::can_start_chat`], counting a local program still looked for,
    /// whose start then says so, rather than offering no chat meanwhile.
    pub(crate) fn may_start_chat(&self) -> bool {
        if !self.config.features.acp_chats {
            return false;
        }
        match self.chat_route() {
            Some(super::Route::Local) => self.chats.program != Program::Missing,
            Some(super::Route::Ssh(_) | super::Route::Remote(_)) => true,
            None => false,
        }
    }

    pub(super) fn chat_route(&self) -> Option<super::Route> {
        self.endpoints
            .get(self.selected_endpoint)
            .and_then(|endpoint| {
                super::discovery::route(&endpoint.connection.target, &self.live).or_else(|| {
                    let host = crate::core_terminals::Host::of(&crate::browser::scope(endpoint));
                    self.core_terminals
                        .answers(&host)
                        .then(|| self.core_chat_route(&host))
                        .flatten()
                })
            })
    }

    /// The line that starts a chat with `agent` in a shell Herdr already
    /// opened on the selected host, such as a new worktree's root pane.
    pub(crate) fn chat_line(&self, agent: Agent) -> Result<String, Error> {
        if !self.config.features.acp_chats {
            return Err(Error::Disabled);
        }
        if self.chat_agent_missing(agent) {
            return Err(Error::AgentMissing(agent));
        }
        match self.chat_route().ok_or(Error::NotLocal)? {
            super::Route::Ssh(_) | super::Route::Remote(_) => Ok(remote_command_line(agent)),
            super::Route::Local => shell_line(
                agent,
                self.chats.program.found()?,
                self.chats.program_choice,
                &self.config.chat.env,
            )
            .ok_or(Error::Line),
        }
    }

    /// The `[chat.env]` variables, with `carry`, the settings of the chat
    /// a new one copies, when they fit their bound; else the new chat
    /// starts with its agent's own.
    pub(crate) fn chat_env(&self, carry: Option<&Settings>) -> Cow<'_, BTreeMap<String, String>> {
        let brief = self.chats.brief.as_ref();
        if carry.is_none() && brief.is_none() {
            return Cow::Borrowed(&self.config.chat.env);
        }
        let mut env = self.config.chat.env.clone();
        if let Some(brief) = brief {
            env.insert(
                super::brief::FILE_ENV.to_owned(),
                brief.file.to_string_lossy().into_owned(),
            );
        }
        match carry.map(Settings::encode).transpose() {
            Ok(Some(text)) => {
                env.insert(sheprd_chat_protocol::SETTINGS_ENV.to_owned(), text);
            }
            Ok(None) => {}
            Err(error) => {
                tracing::warn!(%error, "A new chat starts without the settings it copies");
            }
        }
        Cow::Owned(env)
    }

    /// The built-in agent of the chat in pane `key` and the settings it
    /// runs with, for a new chat like it; `None` when no chat this window
    /// shows runs there, or it runs a custom command.
    pub(crate) fn like_chat(&self, key: &str) -> Option<(Agent, Settings)> {
        let transcript = self.chats.pane(key)?.session.transcript();
        let agent = transcript
            .harnesses
            .as_ref()
            .and_then(|harnesses| harnesses.current)
            .or_else(|| {
                Agent::ALL
                    .iter()
                    .copied()
                    .find(|agent| agent.display_name() == transcript.agent)
            })?;
        Some((agent, Settings::of(agent, transcript)))
    }

    pub(crate) fn chat_starting(&self) -> bool {
        self.chats.starting.is_some()
    }

    /// Opens a tab running a chat with `agent` in the workspace of `beside`,
    /// else of the focused pane, in that pane's folder, with the settings
    /// `carry` copies from another chat.
    pub(crate) fn start_chat(
        &mut self,
        agent: Agent,
        beside: Option<&str>,
        carry: Option<&Settings>,
    ) -> Result<(), Error> {
        if !self.config.features.acp_chats {
            return Err(Error::Disabled);
        }
        if self.chat_agent_missing(agent) {
            return Err(Error::AgentMissing(agent));
        }
        let program = match self.chat_route().ok_or(Error::NotLocal)? {
            super::Route::Ssh(_) | super::Route::Remote(_) if self.chats.brief.is_some() => {
                return Err(Error::BriefRemote);
            }
            super::Route::Ssh(_) | super::Route::Remote(_) => None,
            super::Route::Local => Some(self.chats.program.found()?.to_owned()),
        };
        if self.script_slot_busy() {
            return Err(Error::SlotBusy);
        }
        let snapshot = self
            .live
            .snapshot
            .as_deref()
            .filter(|_| self.live.status.is_connected())
            .ok_or(Error::NotConnected)?;
        let pane = beside
            .or(snapshot.focused_pane_id.as_deref())
            .and_then(|id| snapshot.panes.iter().find(|pane| pane.pane_id == id));
        let workspace = pane
            .map(|pane| pane.workspace_id.clone())
            .or_else(|| snapshot.focused_workspace_id.clone())
            .ok_or(Error::NoWorkspace)?;
        let cwd = pane.and_then(|pane| pane.foreground_cwd.clone().or_else(|| pane.cwd.clone()));
        let boot = snapshot.boot_id.clone();
        let params = tab_params(
            &workspace,
            cwd.as_deref(),
            agent,
            program.as_deref(),
            &self.chat_env(carry),
        );
        let endpoint = &self.endpoints[self.selected_endpoint];
        let request = endpoint
            .connection
            .request_script(&boot, Method::TabCreate, params)
            .map_err(|_| Error::NotConnected)?;
        self.chats.last_started = Some(agent);
        self.chats.starting = Some(Starting {
            agent,
            brief: self.chats.brief.clone(),
            remote: program.is_none(),
            boot,
            deadline: Instant::now() + DEADLINE,
            endpoint: (self.selection_epoch, endpoint.generation),
            endpoint_id: endpoint.id.clone(),
            request,
        });
        Ok(())
    }

    /// [`Self::start_chat`], reporting a failure as a flash.
    /// A herdr-core tab in the active group splits with the chat instead.
    pub(crate) fn start_chat_or_flash(&mut self, agent: Agent, cx: &mut Context<Self>) {
        let started = match self.core_chat_group_tab(None, cx) {
            Some(tab) => self.start_core_chat(agent, tab, None, cx),
            None if self.kept_in_front().is_some() => self.start_kept_core_chat(agent, None, cx),
            None => self.start_chat(agent, None, None),
        };
        if let Err(error) = started {
            self.flash_chat_error(&error, cx);
        }
        cx.notify();
    }

    /// Starts a chat from a tab strip's launcher, so its tab opens in that
    /// strip's group as one from the strip's "+" does.
    /// A herdr-core tab the strip shows splits with the chat instead.
    pub(crate) fn start_chat_in(&mut self, agent: Agent, group: GroupId, cx: &mut Context<Self>) {
        if let Some(tab) = self.core_chat_group_tab(Some(group), cx) {
            if let Err(error) = self.start_core_chat(agent, tab, None, cx) {
                self.flash_chat_error(&error, cx);
            }
            cx.notify();
            return;
        }
        let started = if self.kept_in_front().is_some() {
            self.start_kept_core_chat(agent, None, cx)
        } else {
            self.start_chat(agent, None, None)
        };
        match started {
            Ok(()) => self.expect_new_tab_in(group),
            Err(error) => self.flash_chat_error(&error, cx),
        }
        cx.notify();
    }

    fn flash_chat_error(&mut self, error: &Error, cx: &mut Context<Self>) {
        self.show_flash(
            Flash::warning(fl!("chat-start-failed", error = error.localize())),
            cx,
        );
    }

    /// Runs every tick: once the tab exists, types the line that starts the
    /// chat and shows the tab.
    pub(super) fn poll_chat_start(&mut self, cx: &mut Context<Self>) {
        let Some(starting) = &self.chats.starting else {
            return;
        };
        let current = starting.endpoint
            == (
                self.selection_epoch,
                self.endpoints[self.selected_endpoint].generation,
            )
            && self.live.status.is_connected()
            && self
                .live
                .snapshot
                .as_ref()
                .is_some_and(|snapshot| snapshot.boot_id == starting.boot);
        if !current {
            discard_brief(self.chats.starting.take(), cx);
            return self.show_flash(
                Flash::warning(fl!(
                    "chat-start-failed",
                    error = crate::Error::StaleConnection.localize()
                )),
                cx,
            );
        }
        if Instant::now() >= starting.deadline {
            discard_brief(self.chats.starting.take(), cx);
            return self.show_flash(
                Flash::warning(fl!("chat-start-failed", error = Error::Timeout.localize())),
                cx,
            );
        }
        let request = starting.request.clone();
        let Some(result) = self.take_answer(crate::state::Answer::Script, &request) else {
            return;
        };
        let Some(starting) = self.chats.starting.take() else {
            return;
        };
        let typed = result
            .map_err(|error| super::Error::from(Error::Request(error)).into())
            .and_then(|response| {
                crate::worktree_scripts::created_tab(&response).map_err(|error| match error {
                    crate::Error::WorktreeScripts(crate::worktree_scripts::Error::Response) => {
                        super::Error::from(Error::Response).into()
                    }
                    error => error,
                })
            })
            .and_then(|(tab, pane)| {
                let line = if starting.remote {
                    remote_command_line(starting.agent)
                } else {
                    command_line(
                        starting.agent,
                        self.chats.program_choice,
                        starting.brief.as_ref(),
                    )
                };
                self.type_script_line(&starting.boot, &pane, &line)?;
                Ok(tab)
            });
        match typed {
            Ok(tab) => {
                self.navigate_endpoint(&starting.endpoint_id, NavigationTarget::Tab(&tab), cx);
                cx.notify();
            }
            Err(error) => {
                discard_brief(Some(starting), cx);
                self.show_flash(
                    Flash::warning(fl!("chat-start-failed", error = error.localize())),
                    cx,
                );
            }
        }
    }
}

/// Deletes, off the UI thread, the brief of a chat that did not start.
fn discard_brief(starting: Option<Starting>, cx: &App) {
    if let Some(brief) = starting.and_then(|starting| starting.brief) {
        cx.background_executor()
            .spawn(async move { brief.discard() })
            .detach();
    }
}
