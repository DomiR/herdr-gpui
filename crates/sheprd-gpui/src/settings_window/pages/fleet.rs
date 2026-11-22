//! Fleet (`docs/relay-plan.md`, Fleet): this computer's computers and
//! phones, the hosts they share, and who may reach which. Nothing signs
//! but on a person's click here. Removing a member (with why, which
//! decides what goes with it), removing a host, inviting a computer this
//! Sheprd reaches, and sharing a host with the computers ticked ask first,
//! by a second click; creating the fleet and each computer's access switch
//! act at once. The daemon signs; [`Fleet`] carries the log to the other
//! machines.

use super::{SettingsWindow, ui};
use crate::{
    fleet::{Fleet, Standing, flows, plain},
    i18n::fl,
    remote::{Health, Supervisor},
    settings_window::Section,
};
use gpui::{prelude::*, *};
use sheprd_remote_control::control::fleet::{
    HostKind, Kind, MAX_ALSO_MEMBERS, Reason, View, sync::Ask,
};
use std::collections::BTreeSet;

mod joins;
mod scrub;

/// The page's state while it shows.
#[derive(Default)]
pub(in crate::settings_window) struct FleetPage {
    /// The member whose Remove waits for a reason, and the reason picked.
    removing: Option<(String, Option<Reason>)>,
    /// The host whose Remove waits for confirmation.
    removing_host: Option<String>,
    /// The machine whose Invite waits for confirmation.
    inviting: Option<String>,
    /// The machine being shared, with the computers left out of it.
    sharing: Option<(String, BTreeSet<String>)>,
    /// The share installs the relay agent: the machine runs no Sheprd.
    sharing_plain: bool,
    /// The machine whose Remove fleet access waits for confirmation.
    scrubbing: Option<String>,
    /// The joining member whose Admit waits for confirmation.
    admitting: Option<String>,
    /// The last failure, for a person.
    error: Option<String>,
}

/// What Settings > Fleet opens asking, from `sheprd fleet`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Pending {
    Nothing,
    /// Why to remove this member.
    Remove(String),
    /// Whether to remove this host everywhere.
    RemoveHost(String),
    /// Whether to remove every way a fleet reaches this machine.
    Scrub(String),
}

/// Opens Settings at Fleet, asking what `pending` says.
pub(crate) fn open(source: WeakEntity<crate::SheprdWindow>, pending: Pending, cx: &mut App) {
    crate::settings_window::open(source, cx);
    // Queued after `open`'s own deferred work, so the window exists by now.
    cx.defer(move |cx| {
        let Some(handle) = cx
            .default_global::<crate::settings_window::SettingsWindowHandle>()
            .window
        else {
            return;
        };
        let _ = handle.update(cx, |view, window, cx| {
            view.select_section(Section::Fleet, window, cx);
            match pending {
                Pending::Nothing => {}
                Pending::Remove(member) => view.fleet.removing = Some((member, None)),
                Pending::RemoveHost(host) => view.fleet.removing_host = Some(host),
                Pending::Scrub(machine) => view.fleet.scrubbing = Some(machine),
            }
            cx.notify();
        });
    });
}

impl SettingsWindow {
    /// The page shows: read the fleet now.
    pub(in crate::settings_window) fn open_fleet(&mut self, cx: &mut Context<Self>) {
        Fleet::refresh(cx);
    }

    /// The page went away.
    pub(in crate::settings_window) fn close_fleet(&mut self) {
        self.fleet = FleetPage::default();
    }

    /// Runs `ask`, a person here approved, and shows a failure.
    fn fleet_act(&mut self, ask: Ask, cx: &mut Context<Self>) {
        self.fleet.error = None;
        let view = cx.entity().downgrade();
        Fleet::act(cx, ask, move |failure, cx| {
            let _ = view.update(cx, |view, cx| {
                view.fleet.error = failure;
                cx.notify();
            });
        });
    }

    /// Runs `asks` in order, one change a person here approved, and shows
    /// a failure.
    fn fleet_act_all(&mut self, asks: Vec<Ask>, cx: &mut Context<Self>) {
        self.fleet.error = None;
        let view = cx.entity().downgrade();
        Fleet::act_all(cx, asks, move |failure, cx| {
            let _ = view.update(cx, |view, cx| {
                view.fleet.error = failure;
                cx.notify();
            });
        });
    }

    /// With no fleet to show, why one cannot be created yet: the remote
    /// control daemon keeps and signs the log, so with it switched off, or
    /// on but not answering, Create would only fail. A daemon being
    /// started or restarted (`Unknown`, or the supervisor busy) still
    /// offers Create, and a fleet already read stays shown either way.
    fn fleet_needs_remote(&self, cx: &mut Context<Self>) -> Option<ui::Row> {
        let description = match Supervisor::health(cx) {
            Health::Off => fl!("settings-fleet-needs-remote-off"),
            Health::NotRunning(error) if !Supervisor::busy(cx) => fl!(
                "settings-fleet-needs-remote-down",
                error = crate::notifications::safe_text(&error, 300)
            ),
            Health::NotRunning(_) | Health::Unknown | Health::Running(_) => return None,
        };
        let theme = &self.theme;
        let open = ui::button(
            theme,
            "fleet-open-remote",
            fl!("settings-fleet-needs-remote-open"),
            true,
        )
        .on_click(cx.listener(|this, _, window, cx| {
            this.select_section(Section::RemoteControl, window, cx);
        }));
        Some(
            // Where Create shows otherwise, so a search for it lands here.
            ui::row(
                theme,
                "fleet-create-row",
                fl!("settings-fleet-needs-remote"),
            )
            .description(description)
            .control(open),
        )
    }

    pub(in crate::settings_window) fn render_fleet(&self, cx: &mut Context<Self>) -> Div {
        let theme = &self.theme;
        let mut page = div().flex().flex_col();
        if let Some(error) = &self.fleet.error {
            page = page.child(
                ui::row(theme, "fleet-error", fl!("settings-fleet-failed"))
                    .description(crate::notifications::safe_text(error, 300))
                    .dot(theme.ink(theme.palette[1])),
            );
        }
        let Some(view) = Fleet::view(cx).cloned() else {
            if let Some(row) = self.fleet_needs_remote(cx) {
                return page.child(row).children(self.fleet_scrub_only(cx));
            }
            let reading = !Fleet::read(cx);
            let create = ui::button(
                theme,
                "fleet-create",
                fl!("settings-fleet-create"),
                !reading,
            )
            .on_click(cx.listener(|this, _, _, cx| this.fleet_act(Ask::Found, cx)));
            let row = ui::row(theme, "fleet-create-row", fl!("settings-fleet-none"))
                .description(match Fleet::error(cx) {
                    Some(error) => fl!(
                        "settings-fleet-none-failed",
                        error = crate::notifications::safe_text(&error, 300)
                    ),
                    None if reading => fl!("settings-fleet-reading"),
                    None => fl!("settings-fleet-none-description"),
                })
                .control(create);
            return page.child(row).children(self.fleet_scrub_only(cx));
        };
        page = page.children(self.fleet_notices(&view));
        if let Some(unsaved) = Fleet::unsaved(cx) {
            page = page.child(
                ui::row(
                    theme,
                    "fleet-unsaved",
                    fl!("settings-fleet-unsaved", count = unsaved.count),
                )
                .description(crate::notifications::safe_text(&unsaved.reason, 300))
                .dot(theme.ink(theme.palette[1])),
            );
        }
        // A shared host holds the log without being a member: no removal.
        // It signs nothing, so it shows what the fleet lets in here, and
        // can still take fleet access away.
        if view.hosted.is_some() && !view.members.iter().any(|member| member.id == view.me) {
            return page
                .child(
                    ui::row(theme, "fleet-hosted", fl!("settings-fleet-hosted"))
                        .description(fl!("settings-fleet-hosted-description")),
                )
                .child(ui::section(theme, &fl!("settings-fleet-members"), None))
                .children(self.fleet_member_names(&view))
                .children(self.fleet_scrub_only(cx));
        }
        // Removed from the fleet: nothing here counts any more; what it
        // wrote is forgotten once the person says so.
        if !view.members.iter().any(|member| member.id == view.me) {
            let forget = ui::button(theme, "fleet-forget", fl!("settings-fleet-forget"), true)
                .on_click(
                    cx.listener(|this, _, _, cx| this.fleet_act(Ask::Scrub { session: None }, cx)),
                );
            return page
                .child(
                    ui::row(theme, "fleet-removed", fl!("settings-fleet-removed"))
                        .description(fl!("settings-fleet-removed-description"))
                        .dot(theme.ink(theme.palette[1]))
                        .control(forget),
                )
                .children(self.fleet_scrub_only(cx));
        }
        let joins = self.fleet_joins(&view, cx);
        if !joins.is_empty() {
            page = page
                .child(ui::section(theme, &fl!("settings-fleet-joins"), None))
                .children(joins);
        }
        page = page
            .child(ui::section(theme, &fl!("settings-fleet-members"), None))
            .children(self.fleet_members(&view, cx))
            .child(ui::section(theme, &fl!("settings-fleet-hosts"), None))
            .children(self.fleet_hosts(&view, cx))
            .child(ui::section(theme, &fl!("settings-fleet-machines"), None))
            .children(self.fleet_machines(&view, cx));
        page
    }

    /// The members, without actions: what a shared host shows.
    fn fleet_member_names(&self, view: &View) -> Vec<ui::Row> {
        view.members
            .iter()
            .enumerate()
            .map(|(index, member)| {
                ui::row(
                    &self.theme,
                    format!("fleet-member-{index}"),
                    crate::notifications::safe_text(&member.name, 64),
                )
                .description(match member.kind {
                    Kind::Computer => fl!("settings-fleet-kind-computer"),
                    Kind::Phone => fl!("settings-fleet-kind-phone"),
                })
            })
            .collect()
    }

    fn fleet_notices(&self, view: &View) -> Vec<ui::Row> {
        let theme = &self.theme;
        let name = |id: &str| member_name(view, id);
        let mut rows = Vec::new();
        for id in &view.suspended {
            rows.push(
                ui::row(
                    theme,
                    format!("fleet-suspended-{id}"),
                    fl!("settings-fleet-suspended", member = name(id)),
                )
                .description(fl!("settings-fleet-suspended-description"))
                .dot(theme.ink(theme.palette[1])),
            );
        }
        for id in &view.equivocating {
            rows.push(
                ui::row(
                    theme,
                    format!("fleet-equivocating-{id}"),
                    fl!("settings-fleet-equivocating", member = name(id)),
                )
                .description(fl!("settings-fleet-equivocating-description"))
                .dot(theme.ink(theme.palette[1])),
            );
        }
        rows
    }

    fn fleet_members(&self, view: &View, cx: &mut Context<Self>) -> Vec<ui::Row> {
        let theme = &self.theme;
        view.members
            .iter()
            .enumerate()
            .map(|(index, member)| {
                let me = member.id == view.me;
                let kind = match member.kind {
                    Kind::Computer => fl!("settings-fleet-kind-computer"),
                    Kind::Phone => fl!("settings-fleet-kind-phone"),
                };
                let by = match &member.admitted_by {
                    Some(by) => fl!("settings-fleet-admitted-by", member = member_name(view, by)),
                    None => fl!("settings-fleet-founder"),
                };
                let mut description = format!("{kind} \u{b7} {by}");
                if me {
                    description
                        .push_str(&format!(" \u{b7} {}", fl!("settings-fleet-this-computer")));
                }
                let asking = self
                    .fleet
                    .removing
                    .as_ref()
                    .filter(|(id, _)| *id == member.id)
                    .map(|(_, reason)| *reason);
                let mut row = ui::row(
                    theme,
                    format!("fleet-member-{index}"),
                    crate::notifications::safe_text(&member.name, 64),
                )
                .description(match asking {
                    Some(None) => fl!("settings-fleet-remove-why"),
                    Some(Some(reason)) => removal_line(view, &member.id, reason),
                    None => description,
                });
                let id = member.id.clone();
                match asking {
                    None => {
                        let label = if me {
                            fl!("settings-fleet-leave")
                        } else {
                            fl!("settings-fleet-remove")
                        };
                        row = row.control(
                            ui::button(theme, format!("fleet-member-{index}-remove"), label, true)
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.fleet.removing =
                                        Some((id.clone(), me.then_some(Reason::Left)));
                                    cx.notify();
                                })),
                        );
                    }
                    Some(None) => {
                        for (slot, reason) in [Reason::Lost, Reason::Compromised, Reason::Retired]
                            .into_iter()
                            .enumerate()
                        {
                            let id = id.clone();
                            row = row.control(
                                ui::button(
                                    theme,
                                    format!("fleet-member-{index}-reason-{slot}"),
                                    reason_label(reason),
                                    true,
                                )
                                .on_click(cx.listener(
                                    move |this, _, _, cx| {
                                        this.fleet.removing = Some((id.clone(), Some(reason)));
                                        cx.notify();
                                    },
                                )),
                            );
                        }
                        row = row
                            .control(self.fleet_cancel(format!("fleet-member-{index}-cancel"), cx));
                    }
                    Some(Some(reason)) => {
                        let asks = removal(view, &member.id, reason);
                        row = row
                            .control(self.fleet_cancel(format!("fleet-member-{index}-cancel"), cx))
                            .control(
                                ui::button(
                                    theme,
                                    format!("fleet-member-{index}-confirm"),
                                    if me {
                                        fl!("settings-fleet-leave")
                                    } else {
                                        fl!("settings-fleet-remove")
                                    },
                                    true,
                                )
                                .text_color(crate::menu::danger(theme))
                                .on_click(cx.listener(
                                    move |this, _, _, cx| {
                                        this.fleet.removing = None;
                                        this.fleet_act_all(asks.clone(), cx);
                                    },
                                )),
                            );
                    }
                }
                row
            })
            .collect()
    }

    fn fleet_hosts(&self, view: &View, cx: &mut Context<Self>) -> Vec<ui::Row> {
        let theme = &self.theme;
        let computers: Vec<_> = view
            .members
            .iter()
            .filter(|member| member.kind == Kind::Computer)
            .collect();
        let mut rows = Vec::new();
        for (index, host) in view.hosts.iter().enumerate() {
            let own = host.kind == HostKind::Member;
            let mut description = fl!(
                "settings-fleet-host-line",
                member = member_name(view, &host.added_by),
                allowed = host.grants.len()
            );
            if host.held {
                description = fl!("settings-fleet-host-held");
            }
            let asking = self.fleet.removing_host.as_ref() == Some(&host.id);
            let mut row = ui::row(
                theme,
                format!("fleet-host-{index}"),
                crate::notifications::safe_text(&host.name, 64),
            )
            .description(if asking {
                fl!("settings-fleet-remove-host-ask")
            } else {
                description
            });
            if host.held {
                row = row.dot(theme.ink(theme.palette[1]));
            }
            // A computer's own host goes with it, from Members.
            if !own {
                let id = host.id.clone();
                if asking {
                    row = row.control(self.fleet_cancel(format!("fleet-host-{index}-cancel"), cx));
                }
                row = row.control(
                    ui::button(
                        theme,
                        format!("fleet-host-{index}-remove"),
                        fl!("settings-fleet-remove-host"),
                        true,
                    )
                    .when(asking, |button| {
                        button.text_color(crate::menu::danger(theme))
                    })
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if this.fleet.removing_host.as_ref() == Some(&id) {
                            this.fleet.removing_host = None;
                            this.fleet_act(Ask::RemoveHost { host: id.clone() }, cx);
                        } else {
                            this.fleet.removing_host = Some(id.clone());
                            cx.notify();
                        }
                    })),
                );
            }
            rows.push(row);
            // Each computer that may reach it, but the host itself.
            for (slot, member) in computers.iter().enumerate() {
                if host.added_by == member.id && own {
                    continue;
                }
                let allowed = host.grants.contains(&member.id);
                let (host_id, member_id) = (host.id.clone(), member.id.clone());
                rows.push(
                    ui::row(
                        theme,
                        format!("fleet-host-{index}-member-{slot}"),
                        fl!(
                            "settings-fleet-host-member",
                            member = crate::notifications::safe_text(&member.name, 64)
                        ),
                    )
                    .switch(
                        ui::toggle(
                            theme,
                            format!("fleet-host-{index}-member-{slot}-toggle"),
                            allowed,
                            !host.held,
                        )
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.fleet_act(
                                Ask::Access {
                                    host: host_id.clone(),
                                    member: member_id.clone(),
                                    allow: !allowed,
                                },
                                cx,
                            );
                        })),
                    ),
                );
            }
        }
        rows
    }

    fn fleet_machines(&self, view: &View, cx: &mut Context<Self>) -> Vec<ui::Row> {
        let theme = &self.theme;
        let machines = Fleet::machines(cx);
        if machines.is_empty() {
            return vec![
                ui::row(
                    theme,
                    "fleet-machines-none",
                    fl!("settings-fleet-machines-none"),
                )
                .description(fl!("settings-fleet-machines-none-description")),
            ];
        }
        let computers: Vec<_> = view
            .members
            .iter()
            .filter(|member| member.kind == Kind::Computer)
            .cloned()
            .collect();
        let mut rows = Vec::new();
        for (index, (id, machine)) in machines.into_iter().enumerate() {
            let standing = Fleet::standing(cx, &id);
            let acting = Fleet::acting(cx, &id);
            let line = match standing {
                _ if acting => fl!("settings-fleet-machine-working"),
                Standing::Member => fl!("settings-fleet-machine-member"),
                Standing::Agent => fl!("settings-fleet-machine-agent"),
                Standing::Outside => fl!("settings-fleet-machine-outside"),
                Standing::NoDaemon => fl!("settings-fleet-machine-no-daemon"),
                Standing::Unchecked => fl!("settings-fleet-machine-unchecked"),
                Standing::Failed => fl!("settings-fleet-machine-failed"),
                Standing::Removed => fl!("settings-fleet-machine-removed"),
                Standing::Unknown => fl!("settings-fleet-machine-unknown"),
            };
            let mut row = ui::row(
                theme,
                format!("fleet-machine-{index}"),
                crate::notifications::safe_text(&machine.label, 80),
            );
            let sharing = self
                .fleet
                .sharing
                .as_ref()
                .filter(|(sharing, _)| *sharing == id)
                .map(|(_, deny)| deny.clone());
            let inviting = self.fleet.inviting.as_ref() == Some(&id);
            if self.fleet.scrubbing.as_ref() == Some(&id) && !acting {
                rows.push(self.fleet_scrub_confirm(row, index, &id, &machine, cx));
                continue;
            }
            let scrub = self.fleet_scrub_button(index, &id, acting, cx);
            if matches!(
                standing,
                Standing::Member | Standing::Agent | Standing::Unchecked
            ) || acting
            {
                rows.push(row.description(line).control(scrub));
                continue;
            }
            if inviting {
                row = row.description(fl!("settings-fleet-invite-ask"));
                let (machine, id_confirm) = (machine.clone(), id.clone());
                row = row
                    .control(self.fleet_cancel(format!("fleet-machine-{index}-cancel"), cx))
                    .control(
                        ui::button(
                            theme,
                            format!("fleet-machine-{index}-invite-confirm"),
                            fl!("settings-fleet-invite"),
                            true,
                        )
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.fleet.inviting = None;
                            let view = cx.entity().downgrade();
                            flows::invite(
                                cx,
                                id_confirm.clone(),
                                machine.clone(),
                                move |failure, cx| {
                                    let _ = view.update(cx, |view, cx| {
                                        view.fleet.error = failure;
                                        cx.notify();
                                    });
                                },
                            );
                        })),
                    );
                rows.push(row);
                continue;
            }
            if let Some(deny) = sharing {
                let plain_share = self.fleet.sharing_plain;
                rows.push(row.description(if plain_share {
                    fl!("settings-fleet-share-plain-ask")
                } else {
                    fl!("settings-fleet-share-ask")
                }));
                for (slot, member) in computers.iter().enumerate() {
                    let allowed = !deny.contains(&member.id);
                    let member_id = member.id.clone();
                    rows.push(
                        ui::row(
                            theme,
                            format!("fleet-machine-{index}-share-{slot}"),
                            fl!(
                                "settings-fleet-host-member",
                                member = crate::notifications::safe_text(&member.name, 64)
                            ),
                        )
                        .switch(
                            ui::toggle(
                                theme,
                                format!("fleet-machine-{index}-share-{slot}-toggle"),
                                allowed,
                                true,
                            )
                            .on_click(cx.listener(
                                move |this, _, _, cx| {
                                    if let Some((_, deny)) = &mut this.fleet.sharing
                                        && !deny.remove(&member_id)
                                    {
                                        deny.insert(member_id.clone());
                                    }
                                    cx.notify();
                                },
                            )),
                        ),
                    );
                }
                let (machine, id_confirm) = (machine.clone(), id.clone());
                rows.push(
                    ui::row(
                        theme,
                        format!("fleet-machine-{index}-share-actions"),
                        String::new(),
                    )
                    .control(self.fleet_cancel(format!("fleet-machine-{index}-share-cancel"), cx))
                    .control(
                        ui::button(
                            theme,
                            format!("fleet-machine-{index}-share-confirm"),
                            if plain_share {
                                fl!("settings-fleet-share-plain")
                            } else {
                                fl!("settings-fleet-share")
                            },
                            true,
                        )
                        .on_click(cx.listener(move |this, _, _, cx| {
                            let deny = this
                                .fleet
                                .sharing
                                .take()
                                .map(|(_, deny)| deny.into_iter().collect())
                                .unwrap_or_default();
                            let view = cx.entity().downgrade();
                            let done = move |failure, cx: &mut App| {
                                let _ = view.update(cx, |view, cx| {
                                    view.fleet.error = failure;
                                    cx.notify();
                                });
                            };
                            let (id, machine) = (id_confirm.clone(), machine.clone());
                            if std::mem::take(&mut this.fleet.sharing_plain) {
                                plain::share(cx, id, machine, deny, done);
                            } else {
                                flows::share(cx, id, machine, deny, done);
                            }
                        })),
                    ),
                );
                continue;
            }
            if standing == Standing::NoDaemon {
                let plain_id = id.clone();
                rows.push(
                    row.description(line)
                        .control(
                            ui::button(
                                theme,
                                format!("fleet-machine-{index}-share-plain"),
                                fl!("settings-fleet-share-plain"),
                                true,
                            )
                            .on_click(cx.listener(
                                move |this, _, _, cx| {
                                    this.fleet.sharing = Some((plain_id.clone(), BTreeSet::new()));
                                    this.fleet.sharing_plain = true;
                                    this.fleet.inviting = None;
                                    cx.notify();
                                },
                            )),
                        )
                        .control(scrub),
                );
                continue;
            }
            let (invite_id, share_id) = (id.clone(), id.clone());
            row = row
                .description(line)
                .control(
                    ui::button(
                        theme,
                        format!("fleet-machine-{index}-invite"),
                        fl!("settings-fleet-invite"),
                        true,
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.fleet.inviting = Some(invite_id.clone());
                        this.fleet.sharing = None;
                        cx.notify();
                    })),
                )
                .control(
                    ui::button(
                        theme,
                        format!("fleet-machine-{index}-share"),
                        fl!("settings-fleet-share"),
                        true,
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.fleet.sharing = Some((share_id.clone(), BTreeSet::new()));
                        this.fleet.sharing_plain = false;
                        this.fleet.inviting = None;
                        cx.notify();
                    })),
                )
                .control(scrub);
            rows.push(row);
        }
        rows
    }

    fn fleet_cancel(&self, id: String, cx: &mut Context<Self>) -> Stateful<Div> {
        ui::button(&self.theme, id, fl!("settings-fleet-cancel"), true).on_click(cx.listener(
            |this, _, _, cx| {
                this.fleet.removing = None;
                this.fleet.removing_host = None;
                this.fleet.inviting = None;
                this.fleet.sharing = None;
                this.fleet.sharing_plain = false;
                this.fleet.scrubbing = None;
                this.fleet.admitting = None;
                cx.notify();
            },
        ))
    }
}

/// A member's name in `view`, or its short id when it is gone.
fn member_name(view: &View, id: &str) -> String {
    view.members
        .iter()
        .find(|member| member.id == id)
        .map_or_else(
            || id.get(..16).unwrap_or(id).to_owned(),
            |member| crate::notifications::safe_text(&member.name, 64),
        )
}

fn reason_label(reason: Reason) -> String {
    match reason {
        Reason::Lost => fl!("settings-fleet-reason-lost"),
        Reason::Compromised => fl!("settings-fleet-reason-compromised"),
        Reason::Retired => fl!("settings-fleet-reason-retired"),
        Reason::Left => fl!("settings-fleet-leave"),
    }
}

/// What removing `member` for `reason` takes with it: for a compromised
/// member, every member it admitted and host it added; otherwise nothing.
/// One removal names at most [`MAX_ALSO_MEMBERS`] members beside its
/// target, so the members it admitted past those are removed by more
/// removals, each naming one of them and up to as many others: none is
/// left out.
pub(in crate::settings_window) fn removal(view: &View, member: &str, reason: Reason) -> Vec<Ask> {
    if reason != Reason::Compromised {
        return vec![Ask::Remove {
            member: member.to_owned(),
            also_members: Vec::new(),
            also_hosts: Vec::new(),
            reason,
        }];
    }
    let admitted: Vec<String> = view
        .members
        .iter()
        .filter(|other| other.admitted_by.as_deref() == Some(member) && other.id != view.me)
        .map(|other| other.id.clone())
        .collect();
    let (first, rest) = admitted.split_at(admitted.len().min(MAX_ALSO_MEMBERS));
    let mut asks = vec![Ask::Remove {
        member: member.to_owned(),
        also_members: first.to_vec(),
        also_hosts: view
            .hosts
            .iter()
            .filter(|host| host.added_by == member && host.kind != HostKind::Member)
            .map(|host| host.id.clone())
            .collect(),
        reason,
    }];
    asks.extend(rest.chunks(MAX_ALSO_MEMBERS + 1).filter_map(|chunk| {
        let (target, also) = chunk.split_first()?;
        Some(Ask::Remove {
            member: target.clone(),
            also_members: also.to_vec(),
            also_hosts: Vec::new(),
            reason,
        })
    }));
    asks
}

/// What the confirmation says goes.
fn removal_line(view: &View, member: &str, reason: Reason) -> String {
    let (mut members, mut hosts) = (0, 0);
    for ask in removal(view, member, reason) {
        if let Ask::Remove {
            member: target,
            also_members,
            also_hosts,
            ..
        } = ask
        {
            members += also_members.len() + usize::from(target != member);
            hosts += also_hosts.len();
        }
    }
    if members == 0 && hosts == 0 {
        fl!("settings-fleet-remove-only")
    } else {
        fl!(
            "settings-fleet-remove-with",
            members = members,
            hosts = hosts
        )
    }
}
