//! Why a brief could not be handed to a new chat.

#[derive(Debug, thiserror::Error)]
pub enum Error {
    // keep-sorted start
    #[error("Could not create the brief's file")]
    Create(#[source] std::io::Error),
    #[error("Could not keep the brief's file")]
    Keep(#[source] std::io::Error),
    #[error("This computer's sheprd-acp predates briefs; update Sheprd to start a chat with one")]
    Unsupported,
    #[error("Could not write the brief's file")]
    Write(#[source] std::io::Error),
    // keep-sorted end
}
