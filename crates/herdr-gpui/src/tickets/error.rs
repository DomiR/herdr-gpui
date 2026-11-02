/// Why the native board could not read or change herdr-dog's tickets.
#[derive(Debug, thiserror::Error)]
pub(crate) enum Error {
    #[error("Cannot build the herdr-dog address for {path}: {source}")]
    Address {
        path: String,
        #[source]
        source: url::ParseError,
    },
    #[error("Cannot reach herdr-dog: {0}")]
    Http(#[source] Box<ureq::Error>),
    #[error("Cannot read herdr-dog's answer: {0}")]
    Read(#[source] std::io::Error),
    #[error("herdr-dog's answer is larger than {limit} bytes")]
    TooLarge { limit: u64 },
    #[error("Unexpected answer from herdr-dog: {0}")]
    Json(#[source] serde_json::Error),
    /// herdr-dog's own error envelope; `message` is its bounded text.
    #[error("herdr-dog refused ({status} {code}): {message}")]
    Refused {
        status: u16,
        code: String,
        message: String,
    },
}

pub(crate) type Result<T> = std::result::Result<T, Error>;
