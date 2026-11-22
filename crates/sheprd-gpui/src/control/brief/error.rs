//! Why `chat new` could not read its brief.

use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    // keep-sorted start
    #[error("The brief is empty")]
    Empty,
    #[error("Could not open {}", path.display())]
    Open {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("Could not read the brief")]
    Read(#[source] std::io::Error),
    #[error("The brief is not UTF-8 text")]
    Text,
    #[error(
        "The brief is longer than {} bytes",
        crate::control::protocol::chat::MAX_BRIEF
    )]
    TooLong,
    // keep-sorted end
}
