//! Document agent link failures: `error-doc-link.ftl`.

use super::Localize;
use crate::{
    doc_link::{AgentCommandError, Error, JobStep, RecordError, UnsupportedReason},
    i18n::fl,
};
use std::path::Path;

fn path(path: &Path) -> String {
    path.display().to_string()
}

fn step(step: JobStep) -> &'static str {
    match step {
        JobStep::CreateTab => "create-tab",
        JobStep::PaneQuery => "pane-query",
        JobStep::Prompt => "prompt",
        JobStep::StartAgent => "start-agent",
    }
}

impl Localize for AgentCommandError {
    fn localize(&self) -> String {
        match self {
            // keep-sorted start
            Self::ControlCharacter => fl!("error-doc-link-agent-command-control"),
            Self::Empty => fl!("error-doc-link-agent-command-empty"),
            Self::Kind { kind } => fl!("error-doc-link-agent-command-kind", kind = kind),
            Self::TooManyArguments { limit } => {
                fl!(
                    "error-doc-link-agent-command-too-many-arguments",
                    limit = *limit
                )
            }
            Self::TooManyBytes { limit } => {
                fl!(
                    "error-doc-link-agent-command-too-many-bytes",
                    limit = *limit
                )
            }
            // keep-sorted end
        }
    }
}

impl Localize for RecordError {
    fn localize(&self) -> String {
        match self {
            // keep-sorted start
            Self::ControlCharacter { field } => {
                fl!("error-doc-link-record-control-character", field = *field)
            }
            Self::DuplicatePath => fl!("error-doc-link-record-duplicate-path"),
            Self::EmptyField { field } => {
                fl!("error-doc-link-record-empty-field", field = *field)
            }
            Self::FieldTooLong { field, limit } => fl!(
                "error-doc-link-record-field-too-long",
                field = *field,
                limit = *limit
            ),
            Self::InvalidAgentName => fl!("error-doc-link-record-invalid-agent-name"),
            Self::Malformed(_) => fl!("error-doc-link-record-malformed"),
            Self::PathTooLong { limit } => {
                fl!("error-doc-link-record-path-too-long", limit = *limit)
            }
            Self::RelativePath => fl!("error-doc-link-record-relative-path"),
            // keep-sorted end
        }
    }
}

impl Localize for Error {
    fn localize(&self) -> String {
        match self {
            // keep-sorted start
            Self::AgentCommand { .. } => fl!("error-doc-link-agent-command"),
            Self::AgentGone { .. } => fl!("error-doc-link-agent-gone"),
            Self::Busy => fl!("error-doc-link-busy"),
            Self::Cancelled => fl!("error-doc-link-cancelled"),
            Self::Debounce { seconds } => fl!("error-doc-link-debounce", seconds = *seconds),
            Self::Decode(_) => fl!("error-doc-link-decode"),
            Self::Document(error) => error.localize(),
            Self::DocumentUnavailable { path: file } => {
                fl!("error-doc-link-document-unavailable", path = path(file))
            }
            Self::InvalidAgentName => fl!("error-doc-link-invalid-agent-name"),
            Self::InvalidRecord { .. } => fl!("error-doc-link-invalid-record"),
            Self::NameTaken { .. } => fl!("error-doc-link-name-taken"),
            Self::NotReady { .. } => fl!("error-doc-link-not-ready"),
            Self::Output { step: failed, .. } => {
                fl!("error-doc-link-output", step = step(*failed))
            }
            Self::Schema { version } => fl!("error-doc-link-schema", version = *version),
            Self::Script {
                step: failed,
                source,
            } => fl!(
                "error-doc-link-script",
                step = step(*failed),
                error = source.localize()
            ),
            Self::Storage { path: file, .. } => {
                fl!("error-doc-link-storage", path = path(file))
            }
            Self::TooMany { limit } => fl!("error-doc-link-too-many", limit = *limit),
            Self::Unavailable => fl!("error-doc-link-unavailable"),
            Self::Unsupported { .. } => fl!("error-doc-link-unsupported"),
            // keep-sorted end
        }
    }
}

impl Localize for UnsupportedReason {
    fn localize(&self) -> String {
        match self {
            Self::Host(error) => error.localize(),
            Self::Target => fl!("error-doc-link-unsupported-target"),
        }
    }
}
