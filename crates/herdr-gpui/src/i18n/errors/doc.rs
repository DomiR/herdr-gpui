//! Document failures, from `error-doc.ftl`: the editor's own and those of
//! `herdr-doc`, which has no messages of its own.

use super::Localize;
use crate::{doc_view::Error, i18n::fl};

fn path(path: &std::path::Path) -> String {
    path.display().to_string()
}

impl Localize for Error {
    fn localize(&self) -> String {
        match self {
            // keep-sorted start
            Self::Doc(error) => error.localize(),
            Self::Loading => fl!("error-doc-loading"),
            Self::Missing => fl!("error-doc-missing"),
            Self::NotMarkdown => fl!("error-doc-not-markdown"),
            // keep-sorted end
        }
    }
}

impl Localize for herdr_doc::Error {
    fn localize(&self) -> String {
        use herdr_doc::Error as E;
        match self {
            // keep-sorted start
            E::AmbiguousText { count } => fl!("error-doc-ambiguous-text", count = *count),
            E::AmbiguousTodo { query, count } => {
                fl!(
                    "error-doc-ambiguous-todo",
                    query = query.as_str(),
                    count = *count
                )
            }
            E::BadItem => fl!("error-doc-bad-item"),
            E::BadRange { .. } => fl!("error-doc-bad-range"),
            E::BadVersion => fl!("error-doc-bad-version"),
            E::Conflict(conflict) => fl!(
                "error-doc-conflict",
                current = conflict.current.0,
                expected = conflict.expected.0
            ),
            E::Crdt(error) => fl!("error-doc-crdt", error = error.to_string()),
            E::EmptyOld => fl!("error-doc-empty-old"),
            E::EmptyText => fl!("error-doc-empty-text"),
            E::Encode(error) => fl!("error-doc-crdt", error = error.to_string()),
            E::Io {
                operation,
                path: file,
                source,
            } => fl!(
                "error-doc-io",
                operation = *operation,
                path = path(file),
                error = source.to_string()
            ),
            E::Missing { path: file } => fl!("error-doc-file-missing", path = path(file)),
            E::NoSection { id } => fl!("error-doc-no-section", id = id.as_str()),
            E::NotAFile { path: file } => fl!("error-doc-not-a-file", path = path(file)),
            E::NoText => fl!("error-doc-no-text"),
            E::NoTodo { query } => fl!("error-doc-no-todo", query = query.as_str()),
            E::NotUtf8 { path: file } => fl!("error-doc-not-utf8", path = path(file)),
            E::TooLarge { len, limit } => fl!("error-doc-too-large", len = *len, limit = *limit),
            // keep-sorted end
        }
    }
}
