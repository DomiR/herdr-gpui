//! Why Claude, run once on its own, gave no answer; the phone shows these
//! on a chat's row, and the desktop under a rename field.

#[derive(Debug, thiserror::Error)]
pub enum Error {
    // keep-sorted start
    #[error("Claude was stopped")]
    Cancelled,
    #[error("Claude answered nothing")]
    Empty,
    #[error("Claude stopped with an error")]
    Failed,
    #[error("Claude is not installed on this computer")]
    NoAgent,
    #[error("Claude could not start")]
    Start(#[source] std::io::Error),
    #[error("Claude took too long to answer")]
    TimedOut,
    // keep-sorted end
}
