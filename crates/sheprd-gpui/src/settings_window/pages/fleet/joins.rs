//! Waiting to join: phones that asked to join through this computer and
//! that no pairing approval covers. Admit asks again before it signs, as
//! joining is the one point of trust.

use super::*;
use crate::fleet::joins;

impl SettingsWindow {
    /// A row for each join waiting; none when nothing waits.
    pub(super) fn fleet_joins(&self, view: &View, cx: &mut Context<Self>) -> Vec<ui::Row> {
        let theme = &self.theme;
        let mut rows = Vec::new();
        let waiting = Fleet::joins(cx).to_vec();
        for (index, join) in waiting.iter().enumerate() {
            let asking = self.fleet.admitting.as_ref() == Some(&join.member);
            let mut row = ui::row(
                theme,
                format!("fleet-join-{index}"),
                crate::notifications::safe_text(&join.name, 64),
            )
            .description(if asking {
                fl!("settings-fleet-join-ask")
            } else {
                fl!(
                    "settings-fleet-join-line",
                    device = crate::notifications::safe_text(&join.device, 64)
                )
            });
            if asking {
                row = row.control(self.fleet_cancel(format!("fleet-join-{index}-cancel"), cx));
            }
            let (member, ask) = (join.member.clone(), joins::admit(join));
            row = row.control(
                ui::button(
                    theme,
                    format!("fleet-join-{index}-admit"),
                    fl!("settings-fleet-admit"),
                    view.members.iter().any(|member| member.id == view.me),
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    if this.fleet.admitting.as_ref() == Some(&member) {
                        this.fleet.admitting = None;
                        this.fleet_act(ask.clone(), cx);
                    } else {
                        this.fleet.admitting = Some(member.clone());
                        cx.notify();
                    }
                })),
            );
            rows.push(row);
        }
        rows
    }
}
