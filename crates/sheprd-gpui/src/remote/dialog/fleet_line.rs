//! The approval step's switch, Also add to your fleet
//! (`docs/relay-plan.md`, Fleet, Joining): shown only while this computer
//! is in a fleet, ticked by default. Ticked, the phone approved here is
//! admitted when it asks to join within ten minutes, matched by the
//! device id the daemon gave it, so another phone's join is never
//! admitted through this approval.

use super::*;
use crate::{fleet::Fleet, toggles::switch};
use sheprd_remote_control::control::fleet::Kind;

impl SheprdWindow {
    pub(super) fn toggle_pairing_fleet(&mut self, cx: &mut Context<Self>) {
        let Some(dialog) = self.menu.remote_pairing.as_mut() else {
            return;
        };
        dialog.join_fleet = !dialog.join_fleet;
        cx.notify();
    }

    /// The switch under the matching code, usable until the user answers.
    pub(super) fn pairing_fleet(&self, enabled: bool, cx: &mut Context<Self>) -> Option<Div> {
        let dialog = self.menu.remote_pairing.as_ref()?;
        // Only a member admits anyone.
        Fleet::member_of(cx)?;
        let view = Fleet::view(cx)?;
        let theme = &self.theme;
        let on = dialog.join_fleet;
        let count = |kind| {
            view.members
                .iter()
                .filter(|member| member.kind == kind)
                .count()
        };
        let label = fl!(
            "remote-fleet-join",
            computers = count(Kind::Computer),
            hosts = view.hosts.len()
        );
        let toggle = div()
            .id("remote-pair-fleet")
            .debug_selector(|| "remote-pair-fleet".into())
            .role(Role::Switch)
            .aria_label(SharedString::from(label.clone()))
            .aria_toggled(if on { Toggled::True } else { Toggled::False })
            .flex()
            .items_start()
            .gap(px(8.))
            .px(px(4.))
            .py(px(3.))
            .rounded(px(crate::config::corners::CONTROL))
            .when(enabled, |row| {
                row.cursor_pointer()
                    .hover(|row| row.bg(rgb(theme.active)))
                    .on_click(cx.listener(|this, _, _, cx| {
                        cx.stop_propagation();
                        this.toggle_pairing_fleet(cx);
                    }))
            })
            .when(!enabled, |row| row.opacity(0.5))
            .child(switch(theme, 16., on))
            .child(
                div()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .child(label)
                    .child(
                        div()
                            .text_color(rgb(theme.subtext()))
                            .child(fl!("remote-fleet-join-note")),
                    ),
            );
        Some(
            div()
                .debug_selector(|| "remote-pair-join-fleet".into())
                .child(toggle),
        )
    }
}
