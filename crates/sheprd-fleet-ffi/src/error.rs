use sheprd_remote_control::control::fleet;

/// Why a call answered nothing. Each variant names what failed, never the
/// content, and `kind` gives the app a stable name to match on.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    // keep-sorted start
    #[error("a fleet request is over its bounds")]
    Bounds,
    #[error("the saved fleet log holds operations that no longer check out")]
    Damaged,
    #[error(transparent)]
    Log(#[from] fleet::Error),
    #[error("an operation is signed by another member than this one")]
    NotMine,
    #[error("the saved fleet log is of another fleet")]
    OtherFleet,
    #[error("the fleet call failed unexpectedly")]
    Panicked,
    #[error("a fleet request is not a call")]
    Request(#[from] serde_json::Error),
    #[error("no operation given founds the fleet this phone joined")]
    Unfounded,
    // keep-sorted end
}

impl Error {
    /// The name the app matches on.
    #[must_use]
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Bounds => "bounds",
            Self::Damaged => "damaged",
            Self::Log(_) => "log",
            Self::NotMine => "not_mine",
            Self::OtherFleet => "other_fleet",
            Self::Panicked => "panicked",
            Self::Request(_) => "request",
            Self::Unfounded => "unfounded",
        }
    }

    /// For a refusal by the log, the rule that failed.
    #[must_use]
    pub fn rule(&self) -> Option<&'static str> {
        let Self::Log(error) = self else {
            return None;
        };
        Some(match error {
            fleet::Error::Access(_) => "access",
            fleet::Error::Bounds => "bounds",
            fleet::Error::Checkpoint => "checkpoint",
            fleet::Error::Encoding => "encoding",
            fleet::Error::Fleet => "fleet",
            fleet::Error::Full => "full",
            fleet::Error::Future => "future",
            fleet::Error::Invalid => "invalid",
            fleet::Error::JoinSignature => "join_signature",
            fleet::Error::ParentMissing => "parent_missing",
            fleet::Error::Permission => "permission",
            fleet::Error::Removal(_) => "removal",
            fleet::Error::Rev => "rev",
            fleet::Error::SelfRecord => "self_record",
            fleet::Error::Sequence => "sequence",
            fleet::Error::Signature => "signature",
            fleet::Error::Signer => "signer",
            fleet::Error::Unknown => "unknown",
        })
    }
}

pub type Result<T, E = Error> = std::result::Result<T, E>;
