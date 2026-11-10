//! The notice stack over the terminal area: config warnings and the
//! daemon's announcement, and whether each card is shown.

use super::SheprdWindow;
use gpui::{prelude::*, *};

impl SheprdWindow {
    /// Config warnings, then the daemon's announcement, stacked over the
    /// top-right of the terminal area below the tab strip, whose buttons stay
    /// reachable. A menu page owns the window's attention, and zen mode keeps
    /// the window clear; they wait behind either.
    pub(super) fn render_notices(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.notices_allowed() {
            return None;
        }
        let mut cards = self.config_diagnostic_cards(cx);
        cards.extend(self.announcement_card(cx));
        if cards.is_empty() {
            return None;
        }
        // Spans the terminal area so a narrow window shrinks the cards rather
        // than pushing them off the left edge. Each card sits in its own row:
        // a column only shrinks its children along its height, so a card with
        // a long line kept its full width there. The stack ends above the
        // window's foot, and a card that scrolls gives up height so the cards
        // under it stay on screen; the rest keep theirs. Neither the stack nor
        // a row takes the pointer, so the terminal around the cards scrolls
        // and selects as before.
        Some(
            div()
                .absolute()
                .top(px(self.tab_strip_height() + 8.))
                .bottom(px(8.))
                .left(px(8.))
                .right(px(8.))
                .flex()
                .flex_col()
                .gap(px(8.))
                .children(cards.into_iter().map(|Notice { card, shrinks }| {
                    div()
                        .flex()
                        .justify_end()
                        .map(|row| match shrinks {
                            true => row.min_h_0(),
                            false => row.flex_none(),
                        })
                        .child(card)
                }))
                .into_any_element(),
        )
    }
}

/// A card the notice stack can draw.
#[derive(Clone, Copy, PartialEq)]
pub(crate) enum NoticeCard {
    AppConfig,
    IconFont,
    DaemonConfig,
    Announcement,
}

/// One card in the notice stack, and whether it scrolls, so it can give up
/// height to keep the stack inside the window.
pub(super) struct Notice {
    pub(super) card: AnyElement,
    pub(super) shrinks: bool,
}

impl SheprdWindow {
    fn notices_allowed(&self) -> bool {
        self.menu.page.is_none() && !self.zen_active()
    }

    /// Whether [`Self::render_notices`] draws a card, so the keyboard can
    /// reach the cards as a region of their own.
    pub(super) fn notices_shown(&self) -> bool {
        use NoticeCard::*;
        [AppConfig, IconFont, DaemonConfig, Announcement]
            .into_iter()
            .any(|card| self.notice_shown(card))
    }

    /// Whether [`Self::render_notices`] draws `card`.
    pub(crate) fn notice_shown(&self, card: NoticeCard) -> bool {
        self.notices_allowed()
            && match card {
                NoticeCard::AppConfig => self.gui_config_diagnostic.visible().is_some(),
                NoticeCard::IconFont => self.icon_font_notice.visible().is_some(),
                NoticeCard::DaemonConfig => self
                    .endpoints
                    .get(self.selected_endpoint)
                    .is_some_and(|endpoint| endpoint.config_diagnostic.visible().is_some()),
                NoticeCard::Announcement => self.live.product_announcement().is_some(),
            }
    }
}
