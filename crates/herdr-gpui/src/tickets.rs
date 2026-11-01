//! The activity bar's two Tickets sections, herdr-dog's ticket board across
//! the window's body:
//!
//! - Tickets: herdr-dog's own web UI, which its server and web app serve on
//!   this machine, in a page the window owns outside the browser tab store
//!   (`TabId::TICKETS`). The page is created the first time the section
//!   shows and keeps its state while other sections show. `[tickets] url`
//!   points it elsewhere than herdr-dog's development server.
//! - Tickets (native): a board the app draws itself from herdr-dog's HTTP
//!   API at `[tickets] api` (`native`), still being built beside the web
//!   one: the lanes, each ticket's page with its properties and comments,
//!   changing a ticket's status, creating tickets, commenting and replying,
//!   and opening the worktree a ticket was started in.

mod api;
mod board;
mod edit;
mod error;
mod native;
mod page;
mod page_body;
mod section;
mod view;
mod worktree;

#[cfg(test)]
mod tests;

pub(crate) use error::{Error, Result};
pub(crate) use native::Native;

use crate::browser::WebUrl;

/// Where herdr-dog's web app serves the board by default: its Vite
/// development server, which forwards `/api` to herdr-dog's server. Vite
/// listens on `localhost`, which may be IPv6 only, so not `127.0.0.1`.
const DEFAULT_URL: &str = "http://localhost:7778/tickets";
/// Where herdr-dog's server answers its API by default.
const DEFAULT_API: &str = "http://127.0.0.1:4319/";

#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct Config {
    /// Where herdr-dog's server answers, for the native board. Unset is
    /// [`DEFAULT_API`].
    pub(crate) api: Option<WebUrl>,
    /// Where herdr-dog serves its board. Unset is [`DEFAULT_URL`].
    pub(crate) url: Option<WebUrl>,
}

impl Config {
    /// The board's address.
    pub(crate) fn url(&self) -> Option<WebUrl> {
        self.url
            .clone()
            .or_else(|| WebUrl::try_from(DEFAULT_URL).ok())
    }

    /// herdr-dog's server address.
    pub(crate) fn api(&self) -> Option<WebUrl> {
        self.api
            .clone()
            .or_else(|| WebUrl::try_from(DEFAULT_API).ok())
    }
}
