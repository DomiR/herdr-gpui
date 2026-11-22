//! Why the agent gave no summary; shown on the chat's row as it says.

use sheprd_client::one_shot;

#[derive(Debug, thiserror::Error)]
pub(crate) enum Error {
    // keep-sorted start
    #[error(transparent)]
    Agent(#[from] one_shot::Error),
    #[error("too many summaries are running; try again in a moment")]
    Busy,
    // keep-sorted end
}

impl Error {
    /// How a request that ran an agent answers this failure.
    pub(crate) fn status(&self) -> axum::http::StatusCode {
        use axum::http::StatusCode;
        match self {
            Self::Busy => StatusCode::TOO_MANY_REQUESTS,
            Self::Agent(one_shot::Error::NoAgent) => StatusCode::CONFLICT,
            Self::Agent(one_shot::Error::TimedOut) => StatusCode::GATEWAY_TIMEOUT,
            Self::Agent(
                one_shot::Error::Cancelled
                | one_shot::Error::Empty
                | one_shot::Error::Failed
                | one_shot::Error::Start(_),
            ) => StatusCode::BAD_GATEWAY,
        }
    }
}
