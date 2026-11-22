//! Why a linked host's push status could not be read.

#[derive(Debug, thiserror::Error)]
pub enum Error {
    // keep-sorted start
    #[error("The host's push status is not one this app can read")]
    Answer(#[source] serde_json::Error),
    #[error("Could not run sheprd-remote-control on the host")]
    Run(#[source] sheprd_client::Error),
    #[error("This kind of host cannot be asked")]
    Unsupported,
    // keep-sorted end
}
