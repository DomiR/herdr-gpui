use thiserror::Error;

/// Why a script, a console read, or clearing a site's data handed back no
/// result. A script's own exception is not one: it is an `Outcome`.
#[derive(Debug, Error)]
pub(crate) enum Error {
    // keep-sorted start
    #[error("WebKit handed back an answer the app could not read")]
    Answer(#[from] serde_json::Error),
    #[error("WebKit handed back no answer")]
    NoAnswer,
    #[error("the page at {url} has no site whose data could be cleared")]
    NoSite { url: String },
    #[error("the page was asked off the main thread")]
    NotMainThread,
    /// What WebKit reported, usually a script that does not compile.
    #[error("{message}")]
    Script { message: String },
    #[error("WebKit could not say which page the tab shows")]
    Url(#[source] wry::Error),
    // keep-sorted end
}
