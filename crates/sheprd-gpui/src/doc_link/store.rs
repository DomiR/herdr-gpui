//! Bounded persistence for document agent links.

use super::{Error, RecordError, Result, valid_agent_name};
use crate::{browser::Scope, state_file};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub(crate) const FILE: &str = "doc-agent-links.json";
pub(crate) const MAX_LINKS: usize = 8;
const MAX_BYTES: u64 = 64 * 1024;
const MAX_FIELD_BYTES: usize = 256;
const MAX_PATH_BYTES: usize = 4096;
const VERSION: u32 = 1;

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub(crate) struct Record {
    pub(crate) agent_name: String,
    pub(crate) enabled: bool,
    pub(crate) pane_id: String,
    pub(crate) path: PathBuf,
    pub(crate) scope: Scope,
    pub(crate) session: Option<String>,
    pub(crate) tab_id: String,
    pub(crate) workspace_id: String,
}

#[derive(Deserialize)]
struct Loaded {
    links: Vec<serde_json::Value>,
    version: u32,
}

#[derive(Serialize)]
struct Saved<'a> {
    links: &'a [Record],
    version: u32,
}

pub(crate) fn load(path: &Path) -> Result<Vec<Record>> {
    let bytes = state_file::read(path, MAX_BYTES).map_err(|source| Error::Storage {
        path: path.to_owned(),
        source: Box::new(source),
    })?;
    bytes.as_deref().map_or(Ok(Vec::new()), parse)
}

pub(crate) fn save(path: &Path, links: &[Record]) -> Result<()> {
    let links = retained(links)?;
    validate_links(&links)?;
    state_file::write(
        path,
        &Saved {
            links: &links,
            version: VERSION,
        },
    )
    .map_err(|source| Error::Storage {
        path: path.to_owned(),
        source: Box::new(source),
    })
}

/// Callers keep records in least recently used order. Disabled records give
/// way before an enabled link, while the on-disk list remains bounded.
pub(super) fn retained(links: &[Record]) -> Result<Vec<Record>> {
    if links.iter().filter(|link| link.enabled).count() > MAX_LINKS {
        return Err(Error::TooMany { limit: MAX_LINKS });
    }
    let mut discard = links.len().saturating_sub(MAX_LINKS);
    Ok(links
        .iter()
        .filter(|link| {
            if !link.enabled && discard > 0 {
                discard -= 1;
                false
            } else {
                true
            }
        })
        .cloned()
        .collect())
}

pub(crate) fn parse(bytes: &[u8]) -> Result<Vec<Record>> {
    let loaded: Loaded = serde_json::from_slice(bytes).map_err(Error::Decode)?;
    if loaded.version != VERSION {
        return Err(Error::Schema {
            version: loaded.version,
        });
    }
    if loaded.links.len() > MAX_LINKS {
        return Err(Error::TooMany { limit: MAX_LINKS });
    }
    let mut links = Vec::with_capacity(loaded.links.len());
    for value in loaded.links {
        let record = serde_json::from_value(value).map_err(|source| Error::InvalidRecord {
            reason: RecordError::Malformed(source),
        })?;
        validate_record(&record, &links)?;
        links.push(record);
    }
    Ok(links)
}

fn validate_links(links: &[Record]) -> Result<()> {
    if links.len() > MAX_LINKS {
        return Err(Error::TooMany { limit: MAX_LINKS });
    }
    let mut checked = Vec::with_capacity(links.len());
    for record in links {
        validate_record(record, &checked)?;
        checked.push(record.clone());
    }
    Ok(())
}

fn validate_record(record: &Record, known: &[Record]) -> Result<()> {
    if !record.path.is_absolute() {
        return Err(invalid(RecordError::RelativePath));
    }
    let path = record.path.to_string_lossy();
    if path.len() > MAX_PATH_BYTES {
        return Err(invalid(RecordError::PathTooLong {
            limit: MAX_PATH_BYTES,
        }));
    }
    if known.iter().any(|known| known.path == record.path) {
        return Err(invalid(RecordError::DuplicatePath));
    }
    for (field, value) in [
        ("agent_name", record.agent_name.as_str()),
        ("pane_id", record.pane_id.as_str()),
        ("scope", record.scope.as_str()),
        ("tab_id", record.tab_id.as_str()),
        ("workspace_id", record.workspace_id.as_str()),
    ] {
        validate_field(field, value, field == "agent_name")?;
    }
    if !valid_agent_name(&record.agent_name) {
        return Err(invalid(RecordError::InvalidAgentName));
    }
    if let Some(session) = &record.session {
        validate_field("session", session, true)?;
    }
    if path.chars().any(char::is_control) {
        return Err(invalid(RecordError::ControlCharacter { field: "path" }));
    }
    Ok(())
}

fn validate_field(field: &'static str, value: &str, empty_allowed: bool) -> Result<()> {
    if !empty_allowed && value.is_empty() {
        return Err(invalid(RecordError::EmptyField { field }));
    }
    if value.len() > MAX_FIELD_BYTES {
        return Err(invalid(RecordError::FieldTooLong {
            field,
            limit: MAX_FIELD_BYTES,
        }));
    }
    if value.chars().any(char::is_control) {
        return Err(invalid(RecordError::ControlCharacter { field }));
    }
    Ok(())
}

fn invalid(reason: RecordError) -> Error {
    Error::InvalidRecord { reason }
}
