//! The blocking half of naming: reading terminals, prompting a terminal
//! agent, and running Claude once on its own. Run only on a worker.

use super::Error;
use crate::teleport::Host;
use sheprd_backend::TerminalId;
use sheprd_chat_protocol::{excerpt::MAX_INPUT_CHARS, naming};
use sheprd_client::one_shot;
use sheprd_core_client::CoreBackend;
use std::sync::atomic::{AtomicBool, Ordering};

/// Lines of a Herdr 1 pane's recent output read.
const TERMINAL_LINES: &str = "120";
/// The most of one source Claude reads, in characters, the end kept.
const MAX_SOURCE_CHARS: usize = 4_000;
/// Herdr's refusal of a prompt to an agent that is asking for input.
const BLOCKED: &str = "agent_blocked";

/// How a terminal is reached.
#[derive(Clone)]
pub(super) enum Reach {
    /// A Herdr 1 pane, through the `herdr` CLI on its host.
    Herdr { host: Host, pane: String },
    /// A herdr-core terminal, through its host's core.
    Core {
        backend: CoreBackend,
        terminal: TerminalId,
        /// The agent as the core names it (`claude`), which a prompt must
        /// match: the core types nothing once another program has the
        /// terminal.
        reported: Option<String>,
    },
}

impl std::fmt::Debug for Reach {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Herdr { host, pane } => f
                .debug_struct("Herdr")
                .field("host", host)
                .field("pane", pane)
                .finish(),
            // The core's connection has nothing to show.
            Self::Core {
                terminal, reported, ..
            } => f
                .debug_struct("Core")
                .field("terminal", terminal)
                .field("reported", reported)
                .finish(),
        }
    }
}

/// What naming may do with a terminal.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Role {
    /// Read its text, and ask its agent.
    ReadAndAsk,
    /// Read its text only: a watch-only mirror never types into the
    /// terminal it shows.
    Read,
    /// Ask its agent only: a chat observing it already gives its words.
    Ask,
}

impl Role {
    pub(super) fn reads(self) -> bool {
        matches!(self, Self::ReadAndAsk | Self::Read)
    }

    pub(super) fn asks(self) -> bool {
        matches!(self, Self::ReadAndAsk | Self::Ask)
    }
}

/// A terminal of the subject, and the agent running in it, if any.
#[derive(Clone, Debug)]
pub(super) struct Terminal {
    /// What the agent is called on a button and in messages.
    pub(super) agent: Option<String>,
    pub(super) reach: Reach,
    pub(super) role: Role,
}

impl Terminal {
    /// Whether [`super::Way::Agent`] can ask the agent in it.
    pub(super) fn askable(&self) -> bool {
        self.agent.is_some() && self.role.asks()
    }
}

impl Terminal {
    /// What the agent in it is called, for messages.
    pub(super) fn agent(&self) -> String {
        self.agent.clone().unwrap_or_else(|| "Agent".to_owned())
    }
}

/// What Claude reads: each chat's agent and excerpt, and the terminals.
#[derive(Debug, Default)]
pub(super) struct Reading {
    pub(super) chats: Vec<(String, String)>,
    pub(super) terminals: Vec<Terminal>,
}

/// Blocking: `terminal`'s recent text.
pub(super) fn read(terminal: &Terminal, cancelled: &AtomicBool) -> Result<String, Error> {
    match &terminal.reach {
        Reach::Herdr { host, pane } => {
            let line = Host::herdr_line(&[
                "pane",
                "read",
                pane,
                "--source",
                "recent",
                "--lines",
                TERMINAL_LINES,
                "--format",
                "text",
            ]);
            let bytes = host.capture(&line, cancelled).map_err(Error::Script)?;
            Ok(String::from_utf8_lossy(&bytes).into_owned())
        }
        Reach::Core {
            backend, terminal, ..
        } => backend
            .core()
            .read(terminal.as_str())
            .map(|read| read.text)
            .map_err(Error::Core),
    }
}

/// Blocking: types `prompt` into `terminal`'s agent as a prompt of its own.
/// Herdr and the core refuse an agent that is asking for input, rather than
/// answer its question with it.
pub(super) fn prompt(terminal: &Terminal, prompt: &str) -> Result<(), Error> {
    let agent = terminal.agent();
    match &terminal.reach {
        Reach::Herdr { host, pane } => {
            let line = Host::herdr_line(&["agent", "prompt", pane, prompt]);
            host.capture(&line, &AtomicBool::new(false))
                .map(drop)
                .map_err(|error| match crate::pane_move::refused(&error) {
                    Some((code, _)) if code == BLOCKED => Error::Blocked { agent },
                    _ => Error::Script(error),
                })
        }
        Reach::Core {
            backend,
            terminal,
            reported,
        } => {
            use sheprd_core_client::{
                Error as CoreError,
                reply::{Replied, reply},
                wire::{BLOCKED as CORE_BLOCKED, STATUS_MISMATCH},
            };
            let Some(reported) = reported else {
                return Err(Error::Gone);
            };
            match reply(backend.core(), terminal.as_str(), reported, prompt) {
                Ok(Replied::Sent) => Ok(()),
                Ok(Replied::Gone) => Err(Error::Gone),
                Err(CoreError::Refused {
                    kind: Some(kind), ..
                }) if kind == CORE_BLOCKED || kind == STATUS_MISMATCH => {
                    Err(Error::Blocked { agent })
                }
                Err(error) => Err(Error::Core(error)),
            }
        }
    }
}

/// Blocking: Claude's name for what `reading` holds. A terminal that cannot
/// be read is left out; when nothing at all could be, its failure is told.
pub(super) fn ask_claude(reading: Reading, cancelled: &AtomicBool) -> Result<String, Error> {
    let mut parts = Vec::new();
    for (agent, excerpt) in reading.chats {
        if !excerpt.is_empty() {
            parts.push(format!("Chat with {agent}:\n{}", tail(&excerpt)));
        }
    }
    let mut failure = None;
    for terminal in reading
        .terminals
        .iter()
        .filter(|terminal| terminal.role.reads())
    {
        if cancelled.load(Ordering::Acquire) {
            return Err(Error::Gone);
        }
        match read(terminal, cancelled) {
            Ok(text) if !text.trim().is_empty() => {
                let heading = terminal.agent.as_deref().map_or_else(
                    || "Terminal".to_owned(),
                    |agent| format!("Terminal running {agent}"),
                );
                parts.push(format!("{heading}:\n{}", tail(text.trim())));
            }
            Ok(_) => {}
            Err(error) => failure = failure.or(Some(error)),
        }
    }
    let input = bounded(parts);
    if input.is_empty() {
        return Err(failure.unwrap_or(Error::Empty));
    }
    if cancelled.load(Ordering::Acquire) {
        return Err(Error::Gone);
    }
    let answer = one_shot::ask(naming::TITLE_PROMPT, &input, cancelled)?;
    naming::title_from(&answer).ok_or(Error::Claude(one_shot::Error::Empty))
}

/// The end of `text`, at most [`MAX_SOURCE_CHARS`].
pub(super) fn tail(text: &str) -> &str {
    let count = text.chars().count();
    match text
        .char_indices()
        .nth(count.saturating_sub(MAX_SOURCE_CHARS))
    {
        Some((start, _)) if count > MAX_SOURCE_CHARS => &text[start..],
        _ => text,
    }
}

/// `parts` joined, as many as fit in [`MAX_INPUT_CHARS`], first kept.
pub(super) fn bounded(parts: Vec<String>) -> String {
    let mut input = String::new();
    let mut left = MAX_INPUT_CHARS;
    for part in parts {
        let size = part.chars().count() + 2;
        if size > left {
            break;
        }
        left -= size;
        if !input.is_empty() {
            input.push_str("\n\n");
        }
        input.push_str(&part);
    }
    input
}
