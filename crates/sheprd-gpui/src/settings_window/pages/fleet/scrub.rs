//! Remove fleet access: the last resort for a machine no member can reach
//! or a fleet that is gone. It shows whatever this computer's own standing
//! is, in no fleet or removed from one too, so `sheprd fleet scrub
//! MACHINE` always finds its confirmation.

use super::*;
use crate::fleet::Machine;

impl SettingsWindow {
    /// The machines a window reaches with Remove fleet access alone: what
    /// the page offers for them while this computer counts in no fleet.
    pub(super) fn fleet_scrub_only(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let machines = Fleet::machines(cx);
        if machines.is_empty() {
            return Vec::new();
        }
        let theme = &self.theme;
        let mut rows =
            vec![ui::section(theme, &fl!("settings-fleet-machines"), None).into_any_element()];
        for (index, (id, machine)) in machines.into_iter().enumerate() {
            let acting = Fleet::acting(cx, &id);
            let row = ui::row(
                theme,
                format!("fleet-machine-{index}"),
                crate::notifications::safe_text(&machine.label, 80),
            );
            let row = if self.fleet.scrubbing.as_ref() == Some(&id) && !acting {
                self.fleet_scrub_confirm(row, index, &id, &machine, cx)
            } else {
                let row = if acting {
                    row.description(fl!("settings-fleet-machine-working"))
                } else {
                    row.description(fl!("settings-fleet-scrub-description"))
                };
                row.control(self.fleet_scrub_button(index, &id, acting, cx))
            };
            rows.push(row.into_any_element());
        }
        rows
    }

    /// Remove fleet access on machine `id`, the `index`th: asks first.
    pub(super) fn fleet_scrub_button(
        &self,
        index: usize,
        id: &str,
        acting: bool,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let id = id.to_owned();
        ui::button(
            &self.theme,
            format!("fleet-machine-{index}-scrub"),
            fl!("settings-fleet-scrub"),
            !acting,
        )
        .on_click(cx.listener(move |this, _, _, cx| {
            this.fleet.scrubbing = Some(id.clone());
            this.fleet.inviting = None;
            this.fleet.sharing = None;
            cx.notify();
        }))
    }

    /// `row`, machine `id`'s, asking whether to remove every way a fleet
    /// reaches it.
    pub(super) fn fleet_scrub_confirm(
        &self,
        row: ui::Row,
        index: usize,
        id: &str,
        machine: &Machine,
        cx: &mut Context<Self>,
    ) -> ui::Row {
        let theme = &self.theme;
        let (id, machine) = (id.to_owned(), machine.clone());
        row.description(fl!("settings-fleet-scrub-ask"))
            .control(self.fleet_cancel(format!("fleet-machine-{index}-scrub-cancel"), cx))
            .control(
                ui::button(
                    theme,
                    format!("fleet-machine-{index}-scrub-confirm"),
                    fl!("settings-fleet-scrub"),
                    true,
                )
                .text_color(crate::menu::danger(theme))
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.fleet.scrubbing = None;
                    this.fleet.error = None;
                    let view = cx.entity().downgrade();
                    flows::scrub(cx, id.clone(), machine.clone(), move |failure, cx| {
                        let _ = view.update(cx, |view, cx| {
                            view.fleet.error = failure;
                            cx.notify();
                        });
                    });
                })),
            )
    }
}
