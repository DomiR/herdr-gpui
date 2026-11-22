//! Briefs: prompts another agent wrote, such as the one `sheprd chat new
//! --prompt` starts a chat with (`--brief-file`, `--brief-from`).
//!
//! The chat marks a brief's entry with `Change::Brief` so clients can show
//! it as a document rather than the person's words. The agent's own log has
//! no room for the mark, so a small private JSON file per agent session,
//! beside the restore records, keeps a checksum of each brief's text and who
//! wrote it; a continued session marks the replayed prompts that match. A
//! file that is missing, too large, or no longer decodes marks nothing.

mod error;

pub use error::Error;

use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use sheprd_chat_protocol::{Agent, MAX_BRIEFS, MAX_PROMPT_SIZE};

/// The most a session's file may hold: [`MAX_BRIEFS`] short records.
const MAX_FILE: u64 = 4 * 1024;
/// Session files kept; saving past it removes the oldest.
const MAX_FILES: usize = 512;
/// Directory entries looked at when pruning.
const MAX_ENTRIES: usize = 4096;
/// The longest agent session id a file is named after.
const MAX_SESSION_ID: usize = sheprd_chat_protocol::restore::MAX_SESSION_ID;

/// A brief the chat program was started with.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Brief {
    pub from: Option<Agent>,
    pub text: String,
}

/// A brief known by its text's checksum.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub(crate) struct Known {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<Agent>,
    #[serde(with = "hex")]
    pub sum: u64,
}

impl Known {
    pub(crate) fn of(text: &str, from: Option<Agent>) -> Self {
        Self {
            from,
            sum: checksum(text),
        }
    }
}

#[derive(Debug, Default, Deserialize, Serialize)]
struct File {
    #[serde(default)]
    briefs: Vec<Known>,
}

/// The checksum a brief is known by: of its text as a client shows it,
/// without a paste envelope or surrounding blank space, so the prompt the
/// agent's log replays matches the one sent.
pub(crate) fn checksum(text: &str) -> u64 {
    let shown = sheprd_chat_protocol::display::unwrap_paste(text).unwrap_or(text);
    crate::context::checksum(shown.trim())
}

/// Reads the brief at `path` and removes the file, which the app wrote for
/// this chat alone; a file over [`MAX_PROMPT_SIZE`] is refused.
pub(crate) fn take(path: &Path, from: Option<Agent>) -> Result<Brief, Error> {
    let read = |source| Error::Read {
        path: path.to_owned(),
        source,
    };
    let mut bytes = Vec::new();
    let opened = fs::File::open(path).map_err(read)?;
    let _ = fs::remove_file(path);
    opened
        .take(MAX_PROMPT_SIZE as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(read)?;
    if bytes.len() > MAX_PROMPT_SIZE {
        return Err(Error::TooLong);
    }
    let text = String::from_utf8(bytes).map_err(Error::NotUtf8)?;
    if text.trim().is_empty() {
        return Err(Error::Empty);
    }
    Ok(Brief { from, text })
}

/// Where the files live: `briefs` beside the chats' restore records.
pub(crate) fn dir() -> Option<PathBuf> {
    let restore = sheprd_chat_protocol::restore::restore_dir()?;
    Some(restore.parent()?.join("briefs"))
}

/// The file of `session`, unless its id cannot name one safely.
fn path(dir: &Path, session: &str) -> Option<PathBuf> {
    let safe = !session.is_empty()
        && session.len() <= MAX_SESSION_ID
        && session
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'));
    safe.then(|| dir.join(format!("{session}.json")))
}

/// The briefs kept for `session`.
pub(crate) fn load(dir: &Path, session: &str) -> Vec<Known> {
    let Some(path) = path(dir, session) else {
        return Vec::new();
    };
    let mut bytes = Vec::new();
    let read = fs::File::open(path)
        .and_then(|file| file.take(MAX_FILE + 1).read_to_end(&mut bytes))
        .is_ok();
    if !read || bytes.len() as u64 > MAX_FILE {
        return Vec::new();
    }
    let mut briefs = serde_json::from_slice::<File>(&bytes)
        .map(|file| file.briefs)
        .unwrap_or_default();
    briefs.truncate(MAX_BRIEFS);
    briefs
}

/// Keeps `brief` among `session`'s, replacing the file whole so a reader
/// never sees half of it; the oldest go past [`MAX_BRIEFS`].
pub(crate) fn save(dir: &Path, session: &str, brief: Known) -> Result<(), Error> {
    let Some(target) = path(dir, session) else {
        return Ok(());
    };
    let mut briefs = load(dir, session);
    briefs.retain(|known| known.sum != brief.sum);
    briefs.push(brief);
    let excess = briefs.len().saturating_sub(MAX_BRIEFS);
    briefs.drain(..excess);
    let bytes = serde_json::to_vec(&File { briefs }).map_err(Error::Encode)?;
    let failed = |source| Error::Save {
        path: dir.to_owned(),
        source,
    };
    let mut builder = fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    std::os::unix::fs::DirBuilderExt::mode(&mut builder, 0o700);
    builder.create(dir).map_err(failed)?;
    prune(dir, &target);
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
    written.map_err(failed)
}

/// Removes the oldest session files once [`MAX_FILES`] are kept, sparing
/// `keep`; at most [`MAX_ENTRIES`] directory entries are looked at.
fn prune(dir: &Path, keep: &Path) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let mut files: Vec<_> = entries
        .take(MAX_ENTRIES)
        .filter_map(Result::ok)
        .filter(|entry| entry.path() != keep)
        .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "json"))
        .filter_map(|entry| Some((entry.metadata().ok()?.modified().ok()?, entry.path())))
        .collect();
    // Room for the file about to be written.
    let excess = (files.len() + 1).saturating_sub(MAX_FILES);
    if excess == 0 {
        return;
    }
    files.sort();
    for (_, path) in files.into_iter().take(excess) {
        let _ = fs::remove_file(path);
    }
}

/// A checksum as sixteen hex digits, as the notes write theirs.
mod hex {
    use serde::{Deserialize, Deserializer, Serializer};

    pub(super) fn serialize<S: Serializer>(sum: &u64, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&format!("{sum:016x}"))
    }

    pub(super) fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<u64, D::Error> {
        let text = String::deserialize(deserializer)?;
        u64::from_str_radix(&text, 16).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests;
