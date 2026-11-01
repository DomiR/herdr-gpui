//! `herdr-doc` failures.

use std::{io, path::PathBuf};

pub type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    // keep-sorted start
    #[error("{count} todos match {query:?}; name one by its number")]
    AmbiguousTodo { query: String, count: usize },
    #[error("A todo must be one line of text, at most 4096 characters")]
    BadItem,
    #[error("The edit {start}..{end} is outside the text or splits a character")]
    BadRange { start: usize, end: usize },
    #[error("A version is the number `doc read` reports")]
    BadVersion,
    #[error("The document is at version {}, not {}", .0.current, .0.expected)]
    Conflict(Box<crate::Conflict>),
    #[error("Could not apply a change to the document: {0}")]
    Crdt(#[from] loro::LoroError),
    #[error("Could not encode the document's changes: {0}")]
    Encode(#[from] loro::LoroEncodeError),
    #[error("Could not {operation} {}: {source}", path.display())]
    Io {
        operation: &'static str,
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("{} does not exist; it may have been deleted or moved", path.display())]
    Missing { path: PathBuf },
    #[error("There is no text to add")]
    EmptyText,
    #[error("Text to replace must not be empty")]
    EmptyOld,
    #[error("{count} places match the text to replace; include more of it so one does")]
    AmbiguousText { count: usize },
    #[error("No todo matches {query:?}")]
    NoTodo { query: String },
    #[error("The text to replace is not in the document")]
    NoText,
    #[error("No section has the ID {id:?}; `doc sections` lists them")]
    NoSection { id: String },
    #[error("{} is not a regular file", path.display())]
    NotAFile { path: PathBuf },
    #[error("{} is not UTF-8 text", path.display())]
    NotUtf8 { path: PathBuf },
    #[error("The document would be {len} bytes, over the {limit}-byte limit")]
    TooLarge { len: u64, limit: usize },
    // keep-sorted end
}
