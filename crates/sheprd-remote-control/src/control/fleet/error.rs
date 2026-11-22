/// Why an operation, a record, or a log was refused. Each variant names the
/// rule that failed, never the content, so it can be logged and shown.
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum Error {
    // keep-sorted start
    #[error(transparent)]
    Access(#[from] sheprd_relay_agent::access::Error),
    #[error("a fleet operation is over its bounds")]
    Bounds,
    #[error("a checkpoint does not match the state its history computes")]
    Checkpoint,
    #[error("a fleet operation is not encoded as it must be")]
    Encoding,
    #[error("a fleet operation is for another fleet")]
    Fleet,
    #[error("the fleet log is full")]
    Full,
    #[error("a fleet operation was signed too far in the future")]
    Future,
    #[error("a fleet operation's fields are not valid")]
    Invalid,
    #[error("a joining member did not sign its own admission")]
    JoinSignature,
    #[error("a fleet operation's parents are missing")]
    ParentMissing,
    #[error("a fleet member may not sign this operation")]
    Permission,
    #[error(transparent)]
    Removal(#[from] sheprd_relay_agent::removal::Error),
    #[error("an access rev is not one above the highest its history holds")]
    Rev,
    #[error("a host's record is not signed by the key that pins it")]
    SelfRecord,
    #[error("a fleet operation's sequence number does not follow its signer's")]
    Sequence,
    #[error("a fleet operation's signature does not check out")]
    Signature,
    #[error("a fleet operation's signer is not a member")]
    Signer,
    #[error("a fleet operation names a member or host that does not exist")]
    Unknown,
    // keep-sorted end
}

pub type Result<T, E = Error> = std::result::Result<T, E>;
