//! The brief `sheprd chat new --prompt` or `--prompt-file` starts a chat
//! with, read where the command runs: a file, or standard input for `-`.

mod error;

pub(crate) use error::Error;

use crate::cli::control::BriefSource;
use crate::control::protocol::chat::MAX_BRIEF;
use std::io::Read;

/// The brief `source` names, at most [`MAX_BRIEF`] bytes of UTF-8 with
/// something besides whitespace. Reads standard input for `-`.
pub(crate) fn read(source: BriefSource) -> Result<String, Error> {
    match source {
        BriefSource::Text(text) => check(text),
        BriefSource::File(path) if path == "-" => from(std::io::stdin().lock()),
        BriefSource::File(path) => {
            let file = std::fs::File::open(&path).map_err(|source| Error::Open {
                path: path.clone().into(),
                source,
            })?;
            from(file)
        }
    }
}

/// The brief in `input`.
pub(crate) fn from(input: impl Read) -> Result<String, Error> {
    let mut bytes = Vec::new();
    input
        .take(MAX_BRIEF as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(Error::Read)?;
    if bytes.len() > MAX_BRIEF {
        return Err(Error::TooLong);
    }
    check(String::from_utf8(bytes).map_err(|_| Error::Text)?)
}

fn check(text: String) -> Result<String, Error> {
    if text.len() > MAX_BRIEF {
        return Err(Error::TooLong);
    }
    if text.trim().is_empty() {
        return Err(Error::Empty);
    }
    Ok(text)
}

#[cfg(test)]
mod tests;
