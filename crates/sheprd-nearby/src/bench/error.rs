/// Why a measurement could not start or finish.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    // keep-sorted start
    #[error("the computer went before the run finished")]
    Gone,
    #[error("{option} needs a number, not {value:?}")]
    InvalidNumber { option: &'static str, value: String },
    #[error("{0} needs a value")]
    MissingValue(&'static str),
    #[error("no computer named {name:?} was found in {seconds} s")]
    NotFound { name: String, seconds: u64 },
    #[error("the computer did not get ready to carry frames in {seconds} s")]
    NotReady { seconds: u64 },
    #[error("the report could not be written")]
    Report(#[source] serde_json::Error),
    #[error("Bluetooth is not available here")]
    Unavailable,
    #[error("unknown mode {0:?}: use serve or run")]
    UnknownMode(String),
    #[error("unknown option {0:?}")]
    UnknownOption(String),
    // keep-sorted end
}
