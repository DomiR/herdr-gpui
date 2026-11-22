/// Why a phone's request to join was not kept.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    // keep-sorted start
    #[error("too many devices are waiting to join; try again in a few minutes")]
    Full,
    #[error(transparent)]
    Log(#[from] crate::control::fleet::Error),
    #[error("only a phone joins this way")]
    NotPhone,
    #[error("that fleet key was removed from the fleet; make a new one")]
    Removed,
    #[error("the join signature does not check out")]
    Signature,
    #[error("the device that asked to join is no longer paired")]
    Unpaired,
    // keep-sorted end
}

pub type Result<T, E = Error> = std::result::Result<T, E>;
