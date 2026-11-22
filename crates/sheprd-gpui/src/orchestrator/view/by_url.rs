//! A GitHub pull request the section lists, found by its address, for what
//! names one only that way, such as a chat's rail.

use super::{OrchestratorView, rows::Tab};
use crate::orchestrator::Item;
use gpui::{Context, ScrollStrategy};

impl OrchestratorView {
    /// The listed GitHub pull request at `url`, from any repository.
    pub(crate) fn pull_request_at(&self, url: &str) -> Option<&Item> {
        self.listed()
            .flat_map(|(_, items, _)| items)
            .filter(|item| item.github_pull_request().is_some())
            .find(|item| {
                item.url
                    .as_deref()
                    .is_some_and(|known| known.eq_ignore_ascii_case(url))
            })
    }

    /// Opens the listed pull request at `url` on its conversation, on the
    /// Pull requests list with its row shown, selected, and scrolled to,
    /// whatever the search and filters would hide; false when the section
    /// does not list it.
    pub(crate) fn open_pull_request_at(&mut self, url: &str, cx: &mut Context<Self>) -> bool {
        let Some(key) = self.pull_request_at(url).map(|item| item.key.canonical()) else {
            return false;
        };
        // Before the page opens: another list's tab closes the open page.
        self.show_list(Tab::PullRequests, cx);
        // The row is rebuilt before the page opens, so the page reads the
        // checkout the row shows: with two holding it, the one with a run.
        self.revealed = Some(key.clone());
        self.selected = Some(key.clone());
        self.detail = Some(super::Detail {
            key: key.clone(),
            tab: super::DetailTab::Conversation,
        });
        self.refresh_rows();
        self.open_detail(key.clone(), cx);
        self.scroll
            .scroll_to_item(self.line_of(&key), ScrollStrategy::Center);
        cx.notify();
        true
    }
}
