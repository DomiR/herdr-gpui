//! The push relay (`[remote] push_relay`, `push.sheprd.org`): its switch,
//! the explanation of what the push relay sees that the user agrees to
//! before it turns on, and how the daemon's iPhone pushes leave it now,
//! with how the last alert and Live Activity push went, and each linked
//! host's mode. Turning it off needs no asking. Saving restarts the daemon
//! with or without `--no-push-relay`, as any `[remote]` change does; the
//! status comes from [`Supervisor`]'s polls while this page holds its
//! lease, and the hosts' from its asks over SSH then.
// i18n: migrated

use super::{Edit, SettingsWindow, ui};
use crate::{
    config::preferences::Preference, direction::Logical, i18n::fl, motion, remote::Supervisor,
};
use gpui::{prelude::*, *};
use sheprd_control_protocol::protocol::remote::HostPush;
use sheprd_remote_control::control::push::{Last, Mode, Outcome, PushStatus};
use std::{cell::Cell, rc::Rc, time::Instant};

/// The explanation shown before the push relay turns on: open while the
/// user decides, unfolding and folding as it comes and goes.
#[derive(Default)]
pub(in crate::settings_window) struct Ask {
    toggle: motion::Toggle,
    /// When it last began to open or close.
    since: Option<Instant>,
    /// How open it was when it last began to open, when that reversed a
    /// close still under way.
    from: f32,
    /// Its height as last drawn, which a fold shrinks from.
    height: Rc<Cell<Pixels>>,
}

impl Ask {
    pub(in crate::settings_window) fn is_open(&self) -> bool {
        self.toggle.is_open()
    }

    fn set(&mut self, open: bool, now: Instant) {
        if open == self.toggle.is_open() {
            return;
        }
        self.from = if open { self.toggle.shown(now) } else { 0. };
        self.toggle.set(open, now, false);
        self.since = Some(now);
    }
}

impl SettingsWindow {
    /// The switch: turning off saves at once, turning on asks first.
    pub(in crate::settings_window) fn toggle_push_relay(&mut self, cx: &mut Context<Self>) {
        if self.config.remote.push_relay {
            self.remote.push_relay.set(false, motion::now());
            self.save_preference(Preference::Remote(Edit::PushRelay(false)), cx);
        } else {
            let open = !self.remote.push_relay.is_open();
            self.remote.push_relay.set(open, motion::now());
        }
        cx.notify();
    }

    /// The explanation's Turn on. While another setting saves, the edit
    /// would be refused, so the explanation stays open.
    pub(in crate::settings_window) fn confirm_push_relay(&mut self, cx: &mut Context<Self>) {
        if self.busy() {
            return;
        }
        self.remote.push_relay.set(false, motion::now());
        self.save_preference(Preference::Remote(Edit::PushRelay(true)), cx);
        cx.notify();
    }

    pub(in crate::settings_window) fn cancel_push_relay(&mut self, cx: &mut Context<Self>) {
        self.remote.push_relay.set(false, motion::now());
        cx.notify();
    }

    /// The switch, the explanation while it asks, and the delivery status.
    pub(super) fn remote_push_relay(&self, cx: &mut Context<Self>) -> Div {
        let theme = &self.theme;
        let on = self.config.remote.push_relay;
        let switch = ui::row(theme, "remote-push-relay", fl!("settings-push-relay-title"))
            .description(fl!("settings-push-relay-note"))
            .switch(
                ui::toggle(theme, "remote-push-relay-toggle", on, !self.busy()).when(
                    !self.busy(),
                    |toggle| {
                        toggle.on_click(cx.listener(|this, _, _, cx| this.toggle_push_relay(cx)))
                    },
                ),
            );
        div()
            .flex()
            .flex_col()
            .child(switch)
            .children(self.push_relay_ask(cx))
            .child(self.push_delivery(cx))
    }

    /// The explanation, unfolding while it asks and folding once answered.
    fn push_relay_ask(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let ask = &self.remote.push_relay;
        let since = ask.since?;
        let id = "remote-push-relay-ask";
        if ask.is_open() {
            let card = self.push_relay_card(true, cx);
            return Some(
                motion::unfold(card, id, ask.height.clone(), since, ask.from).into_any_element(),
            );
        }
        let now = motion::now();
        // A fold that has finished is no longer drawn.
        motion::progress(since, now, motion::duration(motion::LEAVE))?;
        let card = self.push_relay_card(false, cx);
        Some(motion::fold(card, id, ask.height.get(), since).into_any_element())
    }

    /// What the push relay sees, never sees, and cannot do, as
    /// `docs/apns-relay-plan.md` words it, with Turn on and Cancel. While
    /// folding away its buttons no longer act, and Turn on waits while
    /// another setting saves.
    fn push_relay_card(&self, live: bool, cx: &mut Context<Self>) -> Stateful<Div> {
        let theme = &self.theme;
        let heading = |text: String| div().font_weight(FontWeight::SEMIBOLD).child(text);
        let block = |title: String, body: String| {
            div()
                .flex()
                .flex_col()
                .child(heading(title))
                .child(self.control_note(body))
        };
        let can_confirm = live && !self.busy();
        let confirm = ui::button(
            theme,
            "remote-push-relay-confirm",
            fl!("settings-push-relay-ask-confirm"),
            can_confirm,
        )
        .when(can_confirm, |button| {
            button.on_click(cx.listener(|this, _, _, cx| this.confirm_push_relay(cx)))
        });
        let cancel = ui::button(
            theme,
            "remote-push-relay-cancel",
            fl!("settings-push-relay-ask-cancel"),
            live,
        )
        .when(live, |button| {
            button.on_click(cx.listener(|this, _, _, cx| this.cancel_push_relay(cx)))
        });
        div()
            .id("remote-push-relay-ask-card")
            .debug_selector(|| "remote-push-relay-ask".into())
            .role(Role::Group)
            .aria_label(fl!("settings-push-relay-ask"))
            .my(px(6.))
            .p(px(12.))
            .flex()
            .flex_col()
            .gap(px(8.))
            .rounded(px(crate::config::corners::PANEL))
            .bg(rgb(theme.primary_wash()))
            .child(heading(fl!("settings-push-relay-ask")))
            .child(self.control_note(fl!("settings-push-relay-ask-why")))
            .child(block(
                fl!("settings-push-relay-ask-sees-title"),
                fl!("settings-push-relay-ask-sees"),
            ))
            .child(block(
                fl!("settings-push-relay-ask-never-title"),
                fl!("settings-push-relay-ask-never"),
            ))
            .child(block(
                fl!("settings-push-relay-ask-cannot-title"),
                fl!("settings-push-relay-ask-cannot"),
            ))
            .child(self.control_note(fl!("settings-push-relay-ask-apple")))
            .child(self.control_note(fl!("settings-push-relay-ask-phone")))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .gap(px(6.))
                    .child(confirm)
                    .child(cancel),
            )
    }

    /// How pushes leave the daemon now, and how the last of each kind went.
    fn push_delivery(&self, cx: &App) -> Div {
        let theme = &self.theme;
        let status = Supervisor::push_status(cx).filter(|_| self.config.remote.enabled);
        let mut row = ui::row(
            theme,
            "remote-push-delivery",
            fl!("settings-push-relay-delivery"),
        )
        .description(delivery_line(status));
        // The dot says what the lines below say in words.
        if let Some(worked) = status.and_then(last_worked) {
            let color = if worked {
                theme.palette[2]
            } else {
                theme.palette[1]
            };
            row = row.dot(theme.ink(color));
        }
        let now = crate::remote::now_ms();
        let hosts = Supervisor::host_pushes(cx);
        div()
            .flex()
            .flex_col()
            .child(row)
            .when(!hosts.is_empty(), |page| {
                page.child(
                    div()
                        .ps(px(16.))
                        .pb(px(6.))
                        .flex()
                        .flex_col()
                        .debug_selector(|| "remote-push-delivery-hosts".into())
                        .child(self.control_note(fl!("settings-push-relay-hosts")))
                        .children(hosts.iter().map(|host| self.control_note(host_line(host)))),
                )
            })
            .when_some(status, |page, status| {
                page.child(
                    div()
                        .ps(px(16.))
                        .pb(px(6.))
                        .flex()
                        .flex_col()
                        .debug_selector(|| "remote-push-delivery-last".into())
                        .child(self.control_note(last_alert(status.alert.as_ref(), now)))
                        .child(self.control_note(last_live(status.live_activity.as_ref(), now))),
                )
            })
    }
}

/// The delivery row's description: the mode, or that it shows once the
/// daemon runs and says.
pub(in crate::settings_window) fn delivery_line(status: Option<&PushStatus>) -> String {
    match status.map(|status| status.mode) {
        None => fl!("settings-push-relay-delivery-unknown"),
        Some(Mode::Off) => fl!("settings-push-relay-delivery-off"),
        Some(Mode::PushRelay) => fl!("settings-push-relay-delivery-push-relay"),
        Some(Mode::Other) => fl!("settings-push-relay-delivery-other"),
    }
}

/// A linked host's line: how its daemon sends pushes, or why it is unknown.
pub(in crate::settings_window) fn host_line(host: &HostPush) -> String {
    let name = host.name.as_str();
    let Some(status) = &host.status else {
        return match host.error {
            Some(_) => fl!("settings-push-relay-host-failed", host = name),
            None => fl!("settings-push-relay-host-unknown", host = name),
        };
    };
    let mode = match status.mode {
        Mode::Off => "off",
        Mode::PushRelay => "push_relay",
        Mode::Other => "other",
    };
    fl!("settings-push-relay-host", host = name, mode = mode)
}

/// Whether the most recent push of either kind worked; `None` before any.
pub(in crate::settings_window) fn last_worked(status: &PushStatus) -> Option<bool> {
    [&status.alert, &status.live_activity]
        .into_iter()
        .flatten()
        .max_by_key(|last| last.at_ms)
        .map(|last| last.outcome.worked())
}

/// The outcome's selector in the `last` messages, and the status a refusal
/// gave.
fn selector(outcome: &Outcome) -> (&'static str, String) {
    match outcome {
        Outcome::Delivered => ("delivered", String::new()),
        Outcome::Gone => ("gone", String::new()),
        Outcome::Limited => ("limited", String::new()),
        // Digits as they are, without grouping.
        Outcome::Refused { status, .. } => ("refused", status.to_string()),
        Outcome::Unreachable => ("unreachable", String::new()),
        Outcome::Unsent => ("unsent", String::new()),
        Outcome::Other => ("other", String::new()),
    }
}

fn ago(last: &Last, now_ms: u64) -> String {
    crate::ago::text(now_ms.saturating_sub(last.at_ms) / 1000)
}

pub(in crate::settings_window) fn last_alert(last: Option<&Last>, now_ms: u64) -> String {
    let Some(last) = last else {
        return fl!("settings-push-relay-last-alert-none");
    };
    let (outcome, status) = selector(&last.outcome);
    fl!(
        "settings-push-relay-last-alert",
        outcome = outcome,
        ago = ago(last, now_ms),
        status = status,
    )
}

pub(in crate::settings_window) fn last_live(last: Option<&Last>, now_ms: u64) -> String {
    let Some(last) = last else {
        return fl!("settings-push-relay-last-live-none");
    };
    let (outcome, status) = selector(&last.outcome);
    fl!(
        "settings-push-relay-last-live",
        outcome = outcome,
        ago = ago(last, now_ms),
        status = status,
    )
}
