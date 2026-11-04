//! Concurrent edits from the user, agents' commands, and the disk.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;

mod disk_files;
mod merge;
mod sections;
mod versions;

pub(crate) fn agent(pane: &str) -> Author {
    Author {
        pane: pane.into(),
        label: format!("agent {pane}"),
    }
}

/// Types `text` at `at` as the user.
pub(crate) fn type_at(doc: &mut Document, at: usize, text: &str) {
    doc.edit(&[Edit::insert(at, text)]).unwrap();
}

/// Writes the document the way the app does: hands out the text, then
/// confirms it.
pub(crate) fn save(doc: &mut Document) -> String {
    let write = doc.to_write();
    let text = write.text().to_owned();
    doc.wrote(write);
    text
}

/// A change made against `version`, in no section.
pub(crate) fn at(version: Version) -> Scope<'static> {
    Scope {
        version,
        section: None,
    }
}
