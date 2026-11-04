//! Configuration for document agent links.

use crate::{doc_link, teleport::AgentKind};
use serde::Deserialize;
use std::time::Duration;

/// `[doc_agent]`: the agent a document's Connect agent starts, and how long typing pauses before a push.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct DocAgentConfig {
    /// argv: `[0]` is an agent kind Herdr's `agent start --kind` accepts, and the rest are its arguments.
    pub agent_command: Vec<String>,
    /// How long typing pauses before a settled change is sent to the agent.
    pub debounce_seconds: u64,
}

impl Default for DocAgentConfig {
    fn default() -> Self {
        Self {
            agent_command: vec!["claude".into()],
            debounce_seconds: 25,
        }
    }
}

impl DocAgentConfig {
    pub(crate) fn kind(&self) -> AgentKind {
        match self
            .agent_command
            .first()
            .and_then(|name| AgentKind::parse(name))
        {
            Some(kind) => kind,
            None => AgentKind::Claude,
        }
    }

    pub(crate) fn args(&self) -> &[String] {
        self.agent_command
            .split_first()
            .map_or(&[] as &[String], |(_, args)| args)
    }

    pub(crate) fn debounce(&self) -> Duration {
        Duration::from_secs(self.debounce_seconds)
    }

    pub(crate) fn checked(self) -> doc_link::Result<Self> {
        const MAX_ARGUMENTS: usize = 64;
        const MAX_BYTES: usize = 8 * 1024;
        if self.agent_command.is_empty() {
            return Err(doc_link::Error::AgentCommand {
                reason: doc_link::AgentCommandError::Empty,
            });
        }
        if self
            .agent_command
            .iter()
            .any(|argument| argument.chars().any(char::is_control))
        {
            return Err(doc_link::Error::AgentCommand {
                reason: doc_link::AgentCommandError::ControlCharacter,
            });
        }
        if self.agent_command.len() > MAX_ARGUMENTS {
            return Err(doc_link::Error::AgentCommand {
                reason: doc_link::AgentCommandError::TooManyArguments {
                    limit: MAX_ARGUMENTS,
                },
            });
        }
        let bytes: usize = self.agent_command.iter().map(String::len).sum();
        if bytes > MAX_BYTES {
            return Err(doc_link::Error::AgentCommand {
                reason: doc_link::AgentCommandError::TooManyBytes { limit: MAX_BYTES },
            });
        }
        if AgentKind::parse(&self.agent_command[0]).is_none() {
            return Err(doc_link::Error::AgentCommand {
                reason: doc_link::AgentCommandError::Kind {
                    kind: self.agent_command[0].clone(),
                },
            });
        }
        if !(1..=300).contains(&self.debounce_seconds) {
            return Err(doc_link::Error::Debounce {
                seconds: self.debounce_seconds,
            });
        }
        Ok(self)
    }
}
