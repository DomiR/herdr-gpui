use std::{io, path::PathBuf};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("could not read the brief {path}")]
    Read {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error(
        "the brief is longer than {} bytes",
        sheprd_chat_protocol::MAX_PROMPT_SIZE
    )]
    TooLong,
    #[error("the brief is not UTF-8 text")]
    NotUtf8(#[source] std::string::FromUtf8Error),
    #[error("the brief is empty")]
    Empty,
    #[error("could not keep the chat's briefs in {path}")]
    Save {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("could not encode the chat's briefs")]
    Encode(#[source] serde_json::Error),
}
