//! The brief a new chat starts with (`sheprd chat new --prompt`): another
//! agent's task, which `sheprd-acp` sends as the chat's first prompt and
//! marks as that agent's, so clients show it as a brief to read.
//!
//! The text rides in a private file `sheprd-acp` reads and then deletes,
//! named by `--brief-file`. A Herdr tab's typed line names it through
//! [`FILE_ENV`] in the tab's environment, so the line does not depend on
//! where the file is, as for the program's own path. Only chats on this
//! machine take one: a file here is not on an SSH host.

mod error;

pub(crate) use error::Error;

use sheprd_chat_protocol::Agent;
use std::{io::Write as _, path::PathBuf};

/// The option that names the brief's file to `sheprd-acp`.
pub(crate) const FILE_OPTION: &str = "--brief-file";

/// The variable a Herdr tab's typed line reads the brief's file from.
pub(crate) const FILE_ENV: &str = "SHEPRD_BRIEF_FILE";

/// A brief written to its file, for the chat about to start.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Brief {
    /// The private file holding it, which `sheprd-acp` deletes once read.
    pub(crate) file: PathBuf,
    /// The agent of the chat that asked, when it is one.
    pub(crate) from: Option<Agent>,
}

impl Brief {
    /// Writes `text` to a new private file. Blocking: run it off the UI
    /// thread.
    pub(crate) fn write(text: &str, from: Option<Agent>) -> Result<Self, Error> {
        let mut file = tempfile::Builder::new()
            .prefix("sheprd-brief-")
            .suffix(".md")
            .tempfile()
            .map_err(Error::Create)?;
        file.write_all(text.as_bytes()).map_err(Error::Write)?;
        file.as_file().sync_all().map_err(Error::Write)?;
        let (_, file) = file.keep().map_err(|error| Error::Keep(error.error))?;
        Ok(Self { file, from })
    }

    /// Deletes the file of a brief whose chat did not start. Blocking.
    pub(crate) fn discard(&self) {
        if let Err(error) = std::fs::remove_file(&self.file) {
            tracing::warn!(%error, "A brief no chat read stays in the temporary folder");
        }
    }

    /// The program's options for it, before the agent's name.
    pub(crate) fn args(&self) -> Vec<String> {
        let mut args = vec![
            FILE_OPTION.to_owned(),
            self.file.to_string_lossy().into_owned(),
        ];
        if let Some(from) = self.from {
            args.extend(["--brief-from".to_owned(), from.name().to_owned()]);
        }
        args
    }

    /// Puts [`Self::args`] into a program's `argv`, before its last word,
    /// the agent's name.
    pub(crate) fn insert_into(&self, argv: &mut Vec<String>) {
        let at = argv.len().saturating_sub(1);
        argv.splice(at..at, self.args());
    }

    /// [`Self::args`] as a typed line words them, the file read from
    /// [`FILE_ENV`]: `sh` on Unix, PowerShell on Windows.
    pub(crate) fn line_words(&self) -> String {
        let file = if cfg!(windows) {
            format!("$env:{FILE_ENV}")
        } else {
            format!("\"${FILE_ENV}\"")
        };
        let mut words = format!("{FILE_OPTION} {file} ");
        if let Some(from) = self.from {
            words.push_str(&format!("--brief-from {} ", from.name()));
        }
        words
    }
}

#[cfg(test)]
mod tests;
