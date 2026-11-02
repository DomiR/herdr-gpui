//! The window's Tickets page (`crate::tickets`): one page per window,
//! outside the tab store under the reserved `TabId::TICKETS`, created the
//! first time the Tickets section shows. Presenting it is `view::present`'s.

#[cfg(any(target_os = "macos", windows))]
use super::{Location, TabId};
use crate::{HerdrWindow, sidebar::ActivityPanel};
use gpui::{Context, SharedString, Window};

/// What the window tracks of its Tickets page; the page itself lives with
/// the window's other pages.
#[derive(Debug, Default)]
pub(crate) struct TicketsPage {
    /// Where the page was sent, so a changed `tickets.url` loads again.
    #[cfg(any(target_os = "macos", windows))]
    url: Option<super::WebUrl>,
    /// Why the page could not be created at `url`, shown in its place. It is
    /// tried again only once `tickets.url` changes.
    failed: Option<SharedString>,
    /// Whether the page takes the keyboard once it shows.
    focus: bool,
}

impl TicketsPage {
    pub(crate) fn failed(&self) -> Option<&SharedString> {
        self.failed.as_ref()
    }
}

impl HerdrWindow {
    /// Creates the Tickets page while its section shows, loads it again
    /// when `tickets.url` changed, and hands it the keyboard once it shows.
    /// Creating a page starts web content processes, so this runs from
    /// input and the window's tick, never from render.
    pub(crate) fn ensure_tickets_page(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.activity_section() != Some(ActivityPanel::Tickets) {
            return;
        }
        #[cfg(any(target_os = "macos", windows))]
        {
            let Some(url) = self.config.tickets.url() else {
                return;
            };
            let moved = self.tickets.url.as_ref() != Some(&url);
            let location = Location::Web { url: url.clone() };
            let pages = &mut self.browser.pages;
            if pages.contains(TabId::TICKETS) || pages.is_building(TabId::TICKETS) {
                if moved {
                    pages.load(TabId::TICKETS, &location);
                    self.tickets.url = Some(url);
                }
                if self.tickets.focus && pages.contains(TabId::TICKETS) {
                    self.tickets.focus = false;
                    pages.focus(TabId::TICKETS);
                }
                return;
            }
            if self.tickets.failed.is_some() && !moved {
                return;
            }
            self.tickets.failed = pages
                .ensure_page(TabId::TICKETS, &location, window, cx)
                .err()
                .map(|error| {
                    tracing::warn!(%error, "Cannot show the Tickets page");
                    error.to_string().into()
                });
            self.tickets.url = Some(url);
            cx.notify();
        }
        #[cfg(not(any(target_os = "macos", windows)))]
        let _ = (window, cx);
    }

    /// Shows the Tickets page, with the keyboard, as its section opens.
    pub(crate) fn show_tickets_page(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.tickets.focus = true;
        self.ensure_tickets_page(window, cx);
    }

    /// Takes the keyboard back from the Tickets page as another section, or
    /// Workspaces, shows.
    pub(crate) fn blur_tickets_page(&mut self) {
        self.tickets.focus = false;
        #[cfg(any(target_os = "macos", windows))]
        self.browser.pages.blur(TabId::TICKETS);
    }

    /// The element the Tickets page lays out in, once the window has it.
    #[cfg(any(target_os = "macos", windows))]
    pub(crate) fn tickets_page_element(&self) -> Option<super::page::PageElement> {
        self.browser.pages.element(TabId::TICKETS)
    }
}
