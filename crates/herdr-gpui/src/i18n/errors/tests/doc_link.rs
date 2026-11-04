use super::*;
use crate::doc_link::{AgentCommandError, Error, JobStep, RecordError, UnsupportedReason};
use std::path::PathBuf;

fn json() -> serde_json::Error {
    serde_json::from_str::<u8>("x").unwrap_err()
}

fn client() -> herdr_client::Error {
    herdr_client::Error::ScriptCancelled
}

fn assert_script_localized(errors: &[Error]) {
    for error in errors {
        let Error::Script { step, source } = error else {
            continue;
        };
        crate::i18n::show(&Locale::FALLBACK);
        let english = error.localize().replace(ISOLATION, "");
        assert_eq!(
            english,
            format!("{} failed: {}", step.label(), source.localize())
        );
        crate::i18n::show(&Pseudo::Accented.locale());
        assert!(herdr_i18n::pseudo::is_marked(&error.localize()));
        crate::i18n::show(&Locale::FALLBACK);
    }
}

#[test]
fn every_document_link_error_variant_has_a_message() {
    let steps = [
        JobStep::CreateTab,
        JobStep::PaneQuery,
        JobStep::Prompt,
        JobStep::StartAgent,
    ];
    let scripts: Vec<Error> = steps
        .iter()
        .copied()
        .map(|step| Error::Script {
            step,
            source: client(),
        })
        .collect();
    let mut errors: Vec<Error> = steps
        .iter()
        .copied()
        .map(|step| Error::Output {
            step,
            source: json(),
        })
        .collect();
    errors.extend([
        Error::AgentCommand {
            reason: AgentCommandError::Empty,
        },
        Error::AgentGone { source: client() },
        Error::Busy,
        Error::Cancelled,
        Error::Debounce { seconds: 301 },
        Error::Decode(json()),
        Error::Document(crate::doc_view::Error::Loading),
        Error::DocumentUnavailable {
            path: PathBuf::from("/tmp/todo.md"),
        },
        Error::InvalidAgentName,
        Error::InvalidRecord {
            reason: RecordError::RelativePath,
        },
        Error::NameTaken { source: client() },
        Error::NotReady { source: client() },
        Error::Schema { version: 2 },
        Error::Storage {
            path: PathBuf::from("/tmp/doc-agent-links.json"),
            source: Box::new(crate::Error::StaleConnection),
        },
        Error::TooMany { limit: 8 },
        Error::Unavailable,
        Error::Unsupported {
            reason: UnsupportedReason::Target,
        },
    ]);
    assert_localized(&errors);
    assert_script_localized(&scripts);
}

#[test]
fn every_unsupported_reason_has_a_message() {
    assert_localized(&[
        UnsupportedReason::Host(crate::pane_move::Error::UnsupportedHost),
        UnsupportedReason::Target,
    ]);
}

#[test]
fn unsupported_targets_keep_the_typed_reason_in_the_source_chain() {
    let error = Error::Unsupported {
        reason: UnsupportedReason::Target,
    };
    let source = std::error::Error::source(&error).unwrap();
    assert!(source.downcast_ref::<UnsupportedReason>().is_some());
}

#[test]
fn every_agent_command_reason_has_a_message() {
    assert_localized(&[
        AgentCommandError::ControlCharacter,
        AgentCommandError::Empty,
        AgentCommandError::Kind {
            kind: "unknown".to_owned(),
        },
        AgentCommandError::TooManyArguments { limit: 64 },
        AgentCommandError::TooManyBytes { limit: 8192 },
    ]);
}

#[test]
fn every_record_reason_has_a_message() {
    assert_localized(&[
        RecordError::ControlCharacter { field: "path" },
        RecordError::DuplicatePath,
        RecordError::EmptyField { field: "pane_id" },
        RecordError::FieldTooLong {
            field: "workspace_id",
            limit: 256,
        },
        RecordError::InvalidAgentName,
        RecordError::Malformed(json()),
        RecordError::PathTooLong { limit: 4096 },
        RecordError::RelativePath,
    ]);
}
