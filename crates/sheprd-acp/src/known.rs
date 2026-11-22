//! The settings each built-in agent offered when it last opened a new
//! session on this host: its models, thinking levels, and modes. They change
//! only with the agent's version, so a chat shows them at once while the
//! agent it starts or switches to is still coming up, and the agent's own
//! answer replaces them when its session opens.
//!
//! One small private JSON file per agent beside the restore records. A file
//! that is missing, too large, or no longer decodes is simply not known.

use std::{
    collections::HashMap,
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use sheprd_chat_protocol::{Agent, ConfigOption, Modes};

/// The most a file may hold; a long model list with descriptions fits.
const MAX_FILE: u64 = 256 * 1024;

/// What an agent lets a client set, as its session reported it.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub(crate) struct Settings {
    #[serde(default)]
    pub modes: Option<Modes>,
    #[serde(default)]
    pub options: Vec<ConfigOption>,
}

impl Settings {
    pub(crate) fn is_empty(&self) -> bool {
        self.modes.is_none() && self.options.is_empty()
    }
}

/// The settings known for each agent.
pub(crate) type Known = HashMap<Agent, Settings>;

/// Where the files live: `settings` beside the chats' restore records.
pub(crate) fn dir() -> Option<PathBuf> {
    let restore = sheprd_chat_protocol::restore::restore_dir()?;
    Some(restore.parent()?.join("settings"))
}

fn path(dir: &Path, agent: Agent) -> PathBuf {
    dir.join(format!("{}.json", agent.name()))
}

/// Every agent's settings found in `dir`.
pub(crate) fn load(dir: &Path) -> Known {
    Agent::ALL
        .iter()
        .filter_map(|&agent| Some((agent, read(&path(dir, agent))?)))
        .collect()
}

fn read(path: &Path) -> Option<Settings> {
    let mut bytes = Vec::new();
    fs::File::open(path)
        .ok()?
        .take(MAX_FILE + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    if bytes.len() as u64 > MAX_FILE {
        return None;
    }
    serde_json::from_slice::<Settings>(&bytes)
        .ok()
        .filter(|settings| !settings.is_empty())
}

/// Keeps `settings` as `agent`'s, replacing the file whole so a reader
/// never sees half of it. None, or more than a file holds, forgets the
/// agent's, so no setting it dropped shows again.
pub(crate) fn save(dir: &Path, agent: Agent, settings: &Settings) -> crate::Result<()> {
    let bytes = serde_json::to_vec(settings).map_err(crate::Error::Encode)?;
    if settings.is_empty() || bytes.len() as u64 > MAX_FILE {
        return match fs::remove_file(path(dir, agent)) {
            Err(error) if error.kind() != std::io::ErrorKind::NotFound => Err(error.into()),
            _ => Ok(()),
        };
    }
    let mut builder = fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    std::os::unix::fs::DirBuilderExt::mode(&mut builder, 0o700);
    builder.create(dir)?;
    let target = path(dir, agent);
    let temporary = target.with_extension(format!("tmp-{}", std::process::id()));
    let mut options = fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    let written = options
        .open(&temporary)
        .and_then(|mut file| file.write_all(&bytes))
        .and_then(|()| fs::rename(&temporary, &target));
    if written.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    Ok(written?)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests;
