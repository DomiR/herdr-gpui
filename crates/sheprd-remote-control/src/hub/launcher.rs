//! The `sheprd-acp` a phone's new chats run, and whether it lets them
//! switch to another installed agent later, as the desktop's chats do.

use super::{Error, Hub, Result, on_core};
use sheprd_chat_protocol::Agent;
use sheprd_client::agent_choice::AgentChoice;
use std::{
    path::{Path, PathBuf},
    sync::{Mutex, PoisonError},
    time::SystemTime,
};

/// The program a new chat runs, and the choice it offers.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Launcher {
    pub(crate) program: PathBuf,
    pub(crate) choice: AgentChoice,
}

impl Launcher {
    /// The arguments after the program that start `agent`: plain words,
    /// safe to type into any shell unquoted.
    pub(crate) fn args(&self, agent: Agent) -> Vec<String> {
        self.choice
            .option()
            .map(str::to_owned)
            .into_iter()
            .chain([agent.to_string()])
            .collect()
    }

    /// The program and [`Self::args`].
    pub(crate) fn argv(&self, agent: Agent) -> Vec<String> {
        std::iter::once(self.program.to_string_lossy().into_owned())
            .chain(self.args(agent))
            .collect()
    }
}

/// The last program asked, by path and modification time, so a program
/// replaced in place (an update) is asked again and others are not.
#[derive(Debug, Default)]
pub(super) struct Asked(Mutex<Option<(PathBuf, Option<SystemTime>, AgentChoice)>>);

impl Asked {
    fn get(&self, program: &Path, modified: Option<SystemTime>) -> Option<AgentChoice> {
        let asked = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        asked
            .as_ref()
            .filter(|(path, time, _)| path == program && *time == modified)
            .map(|(.., choice)| *choice)
    }

    fn set(&self, program: PathBuf, modified: Option<SystemTime>, choice: AgentChoice) {
        *self.0.lock().unwrap_or_else(PoisonError::into_inner) = Some((program, modified, choice));
    }
}

impl Hub {
    /// The program new chats run, asked once whether it offers the choice.
    pub(super) async fn launcher(&self) -> Result<Launcher> {
        let program = self.program().ok_or(Error::ProgramMissing)?;
        let asked = &self.asked;
        let path = program.clone();
        let modified = on_core(move || Ok(modified(&path))).await?;
        if let Some(choice) = asked.get(&program, modified) {
            return Ok(Launcher { program, choice });
        }
        let path = program.clone();
        let choice = on_core(move || Ok(AgentChoice::of(&path))).await?;
        asked.set(program.clone(), modified, choice);
        Ok(Launcher { program, choice })
    }
}

fn modified(program: &Path) -> Option<SystemTime> {
    std::fs::metadata(program)
        .and_then(|meta| meta.modified())
        .ok()
}

#[cfg(test)]
mod tests;
