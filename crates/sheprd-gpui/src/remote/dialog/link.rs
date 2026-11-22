//! Copy link under the QR (`control::pairing_link`): a pairing link another
//! Sheprd pastes in Add Device > Nearby Sheprd to pair from any network,
//! through this computer's relays. It is put on the clipboard once, and
//! the line under the button says how long it works and who to send it to.

use super::*;
use sheprd_remote_control::app::pairing::Link;

impl SheprdWindow {
    pub(super) fn copy_pairing_link(&mut self, cx: &mut Context<Self>) {
        if let Some(dialog) = self.menu.remote_pairing.as_mut() {
            dialog.pairing.ask_link();
            cx.notify();
        }
    }

    /// The button and what became of the last click.
    pub(super) fn pairing_link(&self, cx: &mut Context<Self>) -> Option<Div> {
        let dialog = self.menu.remote_pairing.as_ref()?;
        let theme = &self.theme;
        let now = cx.background_executor().now();
        let asked = matches!(dialog.pairing.link(), Link::Asked { .. });
        let note = match dialog.pairing.link() {
            Link::None | Link::Asked { .. } | Link::Ready { .. } => None,
            Link::Copied { until } if *until > now => Some((
                fl!(
                    "remote-pair-link-copied",
                    minutes = until.saturating_duration_since(now).as_secs().div_ceil(60)
                ),
                false,
            )),
            Link::Copied { .. } => Some((fl!("remote-pair-link-expired"), false)),
            Link::NoRelays => Some((fl!("remote-pair-link-no-relays"), true)),
            Link::Failed { message } => Some((
                fl!(
                    "remote-pair-link-failed",
                    error = crate::notifications::safe_text(message, 300)
                ),
                true,
            )),
        };
        let mut copy = button(
            theme,
            "remote-pair-copy-link",
            fl!("remote-pair-copy-link"),
            Tone::Plain,
            !asked,
        );
        if !asked {
            copy = copy.on_click(cx.listener(|this, _, _, cx| {
                cx.stop_propagation();
                this.copy_pairing_link(cx);
            }));
        }
        Some(
            div()
                .debug_selector(|| "remote-pair-link".into())
                .flex()
                .flex_col()
                .items_start()
                .gap(px(4.))
                .child(
                    div()
                        .text_color(rgb(theme.subtext()))
                        .child(fl!("remote-pair-link-intro")),
                )
                .child(copy.bg(rgb(theme.active)))
                .children(note.map(|(text, error)| {
                    div()
                        .debug_selector(|| "remote-pair-link-note".into())
                        .text_color(if error {
                            crate::menu::danger(theme)
                        } else {
                            rgb(theme.subtext())
                        })
                        .child(text)
                })),
        )
    }
}
