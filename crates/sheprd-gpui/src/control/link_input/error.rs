//! Why no pairing link could be read from standard input.

#[derive(Debug, thiserror::Error)]
pub enum Error {
    // keep-sorted start
    #[error("No pairing link on standard input; pipe it in")]
    Empty,
    #[error(transparent)]
    Link(#[from] sheprd_remote_control::control::pairing_link::Error),
    #[error("Could not read the pairing link from standard input")]
    Read(#[source] std::io::Error),
    #[error("Standard input is a terminal; pipe the pairing link in so it is not shown")]
    Terminal,
    #[error("The pairing link on standard input is not text")]
    Text,
    #[error(
        "Standard input holds more than {} bytes; no pairing link is that long",
        super::MAX_INPUT
    )]
    TooLong,
    // keep-sorted end
}
