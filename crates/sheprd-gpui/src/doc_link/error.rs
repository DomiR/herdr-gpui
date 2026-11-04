//! Typed failures for document agent links.

use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum AgentCommandError {
    // keep-sorted start
    #[error("an agent command argument contains a control character")]
    ControlCharacter,
    #[error("the agent command is empty")]
    Empty,
    #[error("unknown agent kind {kind}")]
    Kind { kind: String },
    #[error("the agent command has more than {limit} arguments")]
    TooManyArguments { limit: usize },
    #[error("the agent command exceeds {limit} bytes")]
    TooManyBytes { limit: usize },
    // keep-sorted end
}

#[derive(Debug, thiserror::Error)]
pub enum RecordError {
    // keep-sorted start
    #[error("the {field} field contains a control character")]
    ControlCharacter { field: &'static str },
    #[error("a document path appears more than once")]
    DuplicatePath,
    #[error("the {field} field must not be empty")]
    EmptyField { field: &'static str },
    #[error("the {field} field exceeds {limit} bytes")]
    FieldTooLong { field: &'static str, limit: usize },
    #[error("the agent name is invalid")]
    InvalidAgentName,
    #[error("a saved link record is malformed")]
    Malformed(#[source] serde_json::Error),
    #[error("the document path exceeds {limit} bytes")]
    PathTooLong { limit: usize },
    #[error("the document path must be absolute")]
    RelativePath,
    // keep-sorted end
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JobStep {
    // keep-sorted start
    CreateTab,
    PaneQuery,
    Prompt,
    StartAgent,
    // keep-sorted end
}

impl JobStep {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::CreateTab => "creating the document agent tab",
            Self::PaneQuery => "checking the document agent pane",
            Self::Prompt => "sending the document agent prompt",
            Self::StartAgent => "starting the document agent",
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum UnsupportedReason {
    // keep-sorted start
    #[error(transparent)]
    Host(crate::pane_move::Error),
    #[error("this target has no local Herdr 1 agent API")]
    Target,
    // keep-sorted end
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    // keep-sorted start
    #[error("invalid agent command")]
    AgentCommand {
        #[source]
        reason: AgentCommandError,
    },
    #[error("the document agent is gone")]
    AgentGone {
        #[source]
        source: sheprd_client::Error,
    },
    #[error("the previous document agent operation is still stopping")]
    Busy,
    #[error("the document agent link was cancelled")]
    Cancelled,
    #[error("document agent debounce must be between 1 and 300 seconds, got {seconds}")]
    Debounce { seconds: u64 },
    #[error("saved document agent links are not valid JSON")]
    Decode(#[source] serde_json::Error),
    #[error(transparent)]
    Document(#[from] crate::doc_view::Error),
    #[error("the document at {} could not be loaded", path.display())]
    DocumentUnavailable { path: PathBuf },
    #[error("the document agent name is invalid")]
    InvalidAgentName,
    #[error("a saved document agent link is invalid")]
    InvalidRecord {
        #[source]
        reason: RecordError,
    },
    #[error("the document agent name is already taken")]
    NameTaken {
        #[source]
        source: sheprd_client::Error,
    },
    #[error("the document agent is not ready")]
    NotReady {
        #[source]
        source: sheprd_client::Error,
    },
    #[error("{} failed: unexpected output", step.label())]
    Output {
        step: JobStep,
        #[source]
        source: serde_json::Error,
    },
    #[error("unsupported document agent link schema version {version}")]
    Schema { version: u32 },
    #[error("{} failed", step.label())]
    Script {
        step: JobStep,
        #[source]
        source: sheprd_client::Error,
    },
    #[error("could not access saved document agent links at {}", path.display())]
    Storage {
        path: PathBuf,
        #[source]
        source: Box<crate::Error>,
    },
    #[error("too many document agent links, limit is {limit}")]
    TooMany { limit: usize },
    #[error("the document's Herdr workspace is not connected")]
    Unavailable,
    #[error("Agent links need a local Herdr 1 session")]
    Unsupported {
        #[source]
        reason: UnsupportedReason,
    },
    // keep-sorted end
}

pub(crate) type Result<T, E = Error> = std::result::Result<T, E>;
