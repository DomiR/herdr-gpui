//! Orchestrator failures keep their typed causes and the path or column they
//! concern until they are shown.

use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub(crate) enum Error {
    #[error("No data directory is available for orchestrator state")]
    NoDataDirectory,
    #[error("Could not prepare {}: {source}", path.display())]
    Prepare {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("{} is not a private file or directory owned by this user", path.display())]
    NotPrivate { path: PathBuf },
    #[error("Orchestrator database failed: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("Stored {column} is not valid JSON: {source}")]
    Json {
        column: &'static str,
        #[source]
        source: serde_json::Error,
    },
    #[error("Stored {column} is not a valid time: {value:?}")]
    Time {
        column: &'static str,
        value: String,
        #[source]
        source: chrono::ParseError,
    },
    #[error("Unknown issue provider {0:?}")]
    Provider(String),
    #[error("Not an item key: {0:?}")]
    ItemKey(String),
    #[error("Unknown run owner {0:?}")]
    Owner(String),
    #[error(
        "Shared data is schema {found}; this herdr-gpui understands up to {known}. Read-only until updated."
    )]
    NewerSchema { found: i64, known: i64 },
    #[error("Run {0} belongs to another application")]
    NotOwner(String),
    #[error("Run {0} was not found")]
    RunNotFound(String),
    #[error("{operation} failed: {source}")]
    Script {
        operation: &'static str,
        #[source]
        source: herdr_client::Error,
    },
    #[error("{operation} returned unexpected output")]
    Output { operation: &'static str },
    #[error("{operation} returned invalid JSON: {source}")]
    OutputJson {
        operation: &'static str,
        #[source]
        source: serde_json::Error,
    },
    #[error("This host cannot run the orchestrator's scripts")]
    UnsupportedHost,
    #[error("A saved Orchestrator tab names a folder that is not an absolute path")]
    InvalidCheckout,
    #[error("This folder is not inside a Git repository")]
    NotARepository,
    #[error("Remote URL {0:?} names no repository")]
    RemoteUrl(String),
    #[error("Beads is not installed on this host (bd was not found)")]
    BeadsMissing,
    #[error("Bead id {0:?} is not one bd accepts")]
    BeadId(String),
    #[error("GitHub request failed: {0}")]
    Github(#[source] Box<crate::Error>),
    #[error("Cancelled")]
    Cancelled,
    #[error("Could not start the orchestrator's worker: {0}")]
    Worker(#[source] std::io::Error),
    #[error("Several actions are already running; try again when one finishes")]
    Busy,
    #[error("This run has no Herdr workspace to show")]
    NothingToOpen,
    #[error("This run's workspace is closed")]
    WorkspaceClosed,
    #[error("Close the open menu first")]
    MenuOpen,
    #[error("Only web addresses open here")]
    NotWebAddress,
    #[error("Show this repository's host to dispatch elsewhere")]
    HostNotShown,
    #[error("That host cannot take this repository")]
    HostCannotTake,
    #[error("Setting up the other host failed: {0}")]
    Teleport(#[source] crate::teleport::Error),
    #[error("The pull request changed since you confirmed; look at it again before merging")]
    PullRequestChanged,
    #[error("Could not read or save the Linear API key: {0}")]
    LinearKey(#[source] Box<crate::Error>),
    #[error("LINEAR_API_KEY is set but is not a valid Linear API key")]
    LinearVariable,
    #[error("That is not a Linear API key")]
    LinearKeyFormat,
    #[error("Linear did not accept the API key; connect Linear with a new one")]
    LinearKeyRejected,
    #[error("Linear is limiting requests; the list syncs again later")]
    LinearRateLimited,
    #[error(
        "Linear request failed with HTTP {status}{}",
        .message.as_deref().map(|message| format!(": {message}")).unwrap_or_default()
    )]
    LinearStatus {
        status: u16,
        /// Linear's own reason, cleaned and bounded.
        message: Option<String>,
    },
    #[error("Could not reach Linear: {0}")]
    LinearNetwork(#[source] Box<ureq::Error>),
    #[error("Could not read Linear's response: {0}")]
    LinearRead(#[source] std::io::Error),
    #[error("Linear's response was larger than this app reads")]
    LinearSize,
    #[error("The Linear API key cannot be sent in a request header")]
    LinearHeader(#[source] ureq::http::header::InvalidHeaderValue),
}

/// `map_err` adapter naming the host script that failed.
pub(crate) fn script(operation: &'static str) -> impl FnOnce(herdr_client::Error) -> Error {
    move |source| match source {
        herdr_client::Error::ScriptCancelled => Error::Cancelled,
        source => Error::Script { operation, source },
    }
}

pub(crate) type Result<T, E = Error> = std::result::Result<T, E>;
