//! A pairing link pasted on Nearby Sheprd (`docs/relay-plan.md`, Adding a
//! Sheprd from a link): for a computer on any network, out of Bluetooth
//! range. Its Pair device copied the link; here it is checked, then handed
//! to `desktop-pair --link`, and the page goes on as after a code exchange:
//! the fleet choice, the claim, the code to compare, and the save.

use super::{Found, NearbySetup, Step, next_attempt};
use crate::SheprdWindow;
use crate::i18n::{errors::Localize as _, fl};
use crate::search_input::SearchInput;
use gpui::{prelude::*, *};
use sheprd_remote_control::control::pairing_link::PairingLink;

/// The field a link is pasted into.
pub(in crate::menu::devices) fn input(
    view: &SheprdWindow,
    cx: &mut Context<SheprdWindow>,
) -> Entity<SearchInput> {
    let input = cx.new(SearchInput::new);
    input.update(cx, |input, cx| {
        input.set_appearance(view.config.ui.clone(), view.theme.clone(), cx);
        input.set_placeholder(&fl!("devices-nearby-link-placeholder"), cx);
    });
    input
}

impl SheprdWindow {
    /// Opens Add Device > Nearby Sheprd with `link` in its field, for the
    /// user to pair with a click (`sheprd remote add-link`). Whether Add
    /// Device could open here.
    pub(crate) fn open_pairing_link(
        &mut self,
        link: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.device_setup_unavailable().is_some() || !self.open_menu(window, cx) {
            return false;
        }
        self.open_device_setup(window, cx);
        self.open_source(crate::menu::devices::rail::Source::Nearby, window, cx);
        let Some(setup) = &self.menu.nearby_setup else {
            return false;
        };
        let input = setup.link.clone();
        input.update(cx, |input, cx| input.set_text_at(link, link.len(), cx));
        window.focus(&input.read(cx).focus.clone(), cx);
        cx.notify();
        true
    }

    /// Pairs with the computer the pasted link names, when it is one.
    pub(super) fn pair_from_link(&mut self, cx: &mut Context<Self>) {
        let Some(setup) = &mut self.menu.nearby_setup else {
            return;
        };
        if setup.step != Step::Looking {
            return;
        }
        let text = setup.link.read(cx).text().trim().to_owned();
        if text.is_empty() {
            return;
        }
        let link = match text.parse::<PairingLink>() {
            Ok(link) => link,
            Err(error) => {
                setup.error = Some(error.localize());
                cx.notify();
                return;
            }
        };
        // The link is a secret until claimed: it stays only with the child.
        setup
            .link
            .update(cx, |input, cx| input.set_text_at("", 0, cx));
        setup.error = None;
        setup.attempt = next_attempt();
        let found = Found {
            peripheral: String::new(),
            name: crate::notifications::safe_text(&link.computer, 64),
        };
        self.start_nearby_pairing(found, Some(text), cx);
        cx.notify();
    }

    /// The paste field and its button, under the computers found.
    pub(super) fn nearby_link(&self, setup: &NearbySetup, cx: &mut Context<Self>) -> Div {
        let theme = &self.theme;
        div()
            .debug_selector(|| "nearby-setup-link".into())
            .flex_none()
            .flex()
            .flex_col()
            .gap(px(8.))
            .pt(px(12.))
            .border_t_1()
            .border_color(rgb(theme.active))
            .child(
                div()
                    .text_color(rgb(theme.subtext()))
                    .child(fl!("devices-nearby-link-intro")),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .child(div().flex_1().min_w_0().child(setup.link.clone()))
                    .child(
                        div()
                            .id("nearby-setup-link-pair")
                            .debug_selector(|| "nearby-setup-link-pair".into())
                            .role(Role::Button)
                            .aria_label(SharedString::from(fl!("devices-nearby-link-pair")))
                            .flex_none()
                            .px(px(12.))
                            .py(px(6.))
                            .rounded(px(crate::config::corners::CONTROL))
                            .bg(rgb(theme.active))
                            .cursor_pointer()
                            .child(fl!("devices-nearby-link-pair"))
                            .on_click(cx.listener(|this, _, _, cx| {
                                cx.stop_propagation();
                                this.pair_from_link(cx);
                            })),
                    ),
            )
    }
}

#[cfg(test)]
mod tests;
