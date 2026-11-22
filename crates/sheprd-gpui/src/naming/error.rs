//! Why an agent gave the rename dialog no name.

#[derive(Debug, thiserror::Error)]
pub enum Error {
    // keep-sorted start
    #[error("{agent} is waiting for an answer; reply to it first")]
    Blocked { agent: String },
    #[error(transparent)]
    Chat(#[from] crate::chat::Error),
    #[error(transparent)]
    Claude(#[from] sheprd_client::one_shot::Error),
    #[error("Cannot reach herdr-core: {0}")]
    Core(#[source] sheprd_core_client::Error),
    #[error("Nothing has been said or shown here yet")]
    Empty,
    #[error("The agent left before it answered")]
    Gone,
    #[error(transparent)]
    Host(#[from] crate::pane_move::Error),
    #[error("{agent} did not suggest a name")]
    NoName { agent: String },
    #[error("Herdr could not reach the pane: {0}")]
    Script(#[source] sheprd_client::Error),
    #[error("{agent} did not answer in time")]
    TimedOut { agent: String },
    // keep-sorted end
}
