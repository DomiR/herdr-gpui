//! Document tab failures.

#[derive(Debug, thiserror::Error)]
pub enum Error {
    // keep-sorted start
    #[error(transparent)]
    Doc(#[from] herdr_doc::Error),
    #[error("Only Markdown files open as documents")]
    NotMarkdown,
    #[error("The document's file is gone; Cmd-S writes it again")]
    Missing,
    #[error("The document is still loading")]
    Loading,
    // keep-sorted end
}
