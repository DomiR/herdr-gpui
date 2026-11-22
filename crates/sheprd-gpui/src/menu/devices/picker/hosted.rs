//! The device footer and picker for a host's layout, which draws them itself:
//! what they show as plain JSON, and what its clicks and keys do. The picker
//! keeps its state here (open, the selected line, the unfolded device), so
//! the Sessions shortcut, the keys, and every choice behave as in the
//! window's own picker. Dialogs it opens (Add Device, a session's editor)
//! stay the window's.

use super::super::super::sessions::{Entry, Row, target_session};
use super::super::super::{Page, colors};
use super::super::{
    DeviceScope, SetupUnavailable, footer_initials, multi_select_hint, rail::add_device_offer,
};
use super::Line;
use crate::i18n::fl;
use crate::sidebar::hosted::hex;
use crate::{Command, SheprdWindow};
use gpui::{Context, Window};
use serde_json::{Value, json};
use sheprd_client::ConnectTarget;

/// The footer's padding at each end, as the window's own.
const FOOTER_PADDING: f32 = 8.;
/// What Settings takes beside the picker: its button and the gap before it.
const SETTINGS_ROOM: f32 = 28. + 6.;

/// What a host's click on a picker line does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineAction {
    /// A click: picks the device or session, or opens Add Device. With
    /// Cmd (Ctrl elsewhere) held, a device is listed too, or no longer.
    Choose,
    /// The trash beside a session: asks before deleting it.
    Delete,
    /// The session chip beside a device: unfolds its sessions or folds them.
    Sessions,
}

impl LineAction {
    /// The action a host's message names.
    pub fn named(name: &str) -> Option<Self> {
        Some(match name {
            "choose" => Self::Choose,
            "delete" => Self::Delete,
            "sessions" => Self::Sessions,
            _ => return None,
        })
    }
}

/// A line's identity apart from its place, so a click on a list that changed
/// under the pointer acts on nothing rather than on what took its place.
fn line_key(window: &SheprdWindow, line: &Line) -> String {
    match line {
        Line::All => "all".to_owned(),
        Line::AddDevice => "add".to_owned(),
        Line::Device(index) => window
            .endpoints
            .get(*index)
            .map_or_else(String::new, |endpoint| format!("device:{}", endpoint.id)),
        Line::Session(Entry::Row { spec, .. }) => format!("session:{}", spec.label),
        Line::Session(_) => String::new(),
    }
}

impl SheprdWindow {
    /// Whether a host's layout draws the open picker, rather than the window:
    /// the list itself, not a session's editor over it.
    pub(crate) fn hosted_picker_open(&self) -> bool {
        self.hosted && self.menu.page == Some(Page::Devices) && self.menu.session_edit.is_none()
    }

    /// The footer, and the picker while it is open, for a host's layout.
    pub(crate) fn hosted_device_picker(&self) -> Value {
        let label = self.device_footer_label();
        let bar = self.activity_bar_shown();
        let hosts = self.endpoints.len();
        let connected = self.device_footer_connected();
        let adding = self.cloud_jobs_pending();
        // The picker's width, which a host gives it outright: the footer's,
        // less its padding, and Settings beside it without the bar.
        let column = self.sidebar_column_width(self.viewport_width).unwrap_or(0.);
        let bar_width = if self.activity_bar_leads() {
            crate::sidebar::BAR_WIDTH
        } else {
            0.
        };
        let room = column - bar_width - FOOTER_PADDING * 2. - if bar { 0. } else { SETTINGS_ROOM };
        let settings = self
            .config
            .keybindings
            .shortcuts(Command::Settings)
            .next()
            .map_or_else(
                || fl!("devices-settings"),
                |shortcut| fl!("devices-settings-shortcut", shortcut = shortcut.to_string()),
            );
        json!({
            "footer": {
                "adding": (adding > 0).then_some(adding),
                "aria": if connected {
                    fl!("devices-picker-connected", device = label.as_str())
                } else {
                    fl!("devices-picker-disconnected", device = label.as_str())
                },
                "connected": connected,
                // Beside the activity bar, which carries Settings, the picker
                // leads with initials and says how many hosts there are.
                "hosts": (bar && hosts > 1).then(|| fl!("devices-hosts", count = hosts)),
                "initials": bar.then(|| footer_initials(&label)),
                "label": label,
                "settings": (!bar).then_some(settings),
                "shown": self.device_footer_shown(),
                "width": room.max(0.),
            },
            // The dot of a connected device, as the window's own is.
            "online": hex(colors::online(&self.theme)),
            "open": self.hosted_picker_open(),
            // The current scope's and session's fill, as the window's own.
            "wash": hex(self.theme.primary_wash()),
            "sheet": self.hosted_picker_open().then(|| self.hosted_sheet()),
        })
    }

    /// The open picker: the activity card, then the lines.
    fn hosted_sheet(&self) -> Value {
        let mut ordinal = 0;
        let mut lines = Vec::new();
        for line in self.picker_lines() {
            let selectable = line.selectable();
            let mut value = self.hosted_line(&line);
            if selectable {
                value["ordinal"] = json!(ordinal);
                value["key"] = json!(line_key(self, &line));
                ordinal += 1;
            }
            lines.push(value);
        }
        json!({
            "activity": self.hosted_activity(),
            "adding": self.hosted_cloud_jobs(),
            "heading": fl!("devices-heading"),
            "hint": multi_select_hint(),
            "lines": lines,
            "selected": self.menu.selected,
        })
    }

    /// One line of the picker, as the window's own paints it.
    fn hosted_line(&self, line: &Line) -> Value {
        match line {
            Line::All => json!({
                "kind": "all",
                "label": fl!("devices-all"),
                "detail": fl!(
                    "devices-all-detail",
                    count = self.endpoints.len(),
                    connected = self
                        .endpoints
                        .iter()
                        .filter(|e| e.live.status.is_connected())
                        .count()
                ),
                "checked": self.device_filter == DeviceScope::All,
                "enabled": true,
            }),
            Line::AddDevice => {
                let unavailable = self.device_setup_unavailable();
                json!({
                    "kind": "add",
                    "label": fl!("devices-add"),
                    "detail": unavailable.map_or_else(
                        || add_device_offer(self.cloud_offered()),
                        SetupUnavailable::text,
                    ),
                    "checked": false,
                    "enabled": unavailable.is_none(),
                })
            }
            Line::Device(index) => {
                let Some(endpoint) = self.endpoints.get(*index) else {
                    return json!({ "kind": "note", "text": "" });
                };
                let chip = match &endpoint.connection.target {
                    // An explicit socket names no session of its own.
                    ConnectTarget::Socket(_) => json!({ "icon": "sessions" }),
                    target => json!({
                        "word": fl!("devices-session-chip"),
                        "session": target_session(target),
                    }),
                };
                let mut chip = chip;
                chip["open"] = json!(self.menu.unfolded_device.as_ref() == Some(&endpoint.id));
                chip["hint"] = json!(self.session_chip_hint(*index, &endpoint.label));
                json!({
                    "kind": "device",
                    "label": endpoint.label,
                    "detail": fl!(
                        "devices-row-detail",
                        place = Self::device_detail(&endpoint.connection.target),
                        state = endpoint.status_label()
                    ),
                    "checked": self.device_filter.picks(&endpoint.id),
                    "enabled": endpoint.enabled,
                    "online": endpoint.live.status.is_connected(),
                    "chip": chip,
                })
            }
            Line::Session(Entry::Row { row, spec }) => {
                let removable = !matches!(row, Row::Add(_));
                json!({
                    "kind": "session",
                    "label": spec.label.to_string(),
                    "detail": spec.detail.to_string(),
                    "checked": spec.checked,
                    "enabled": spec.enabled,
                    "online": removable.then_some(spec.online),
                    // Red while it can be deleted, as the window's trash is.
                    "delete": removable.then(|| self.session_delete_reason(row).is_none()),
                })
            }
            Line::Session(Entry::Muted { text, .. }) => json!({ "kind": "note", "text": text }),
            Line::Session(Entry::Error(text)) => json!({ "kind": "error", "text": text }),
        }
    }

    /// The activity card: every device's agents over the last two hours, as
    /// bars of working under blocked, and how many work and wait now.
    fn hosted_activity(&self) -> Value {
        let tally = crate::devices_overview::Tally::sum(&self.overview_devices());
        let steps = crate::devices_overview::padded(self.devices_overview.history.totals());
        let peak = steps
            .iter()
            .flatten()
            .map(|counts| counts.working + counts.blocked)
            .max()
            .unwrap_or(0);
        let bars: Vec<Value> = steps
            .into_iter()
            .map(|step| {
                step.map_or(
                    Value::Bool(false),
                    |counts| json!({ "working": counts.working, "blocked": counts.blocked }),
                )
            })
            .collect();
        json!({
            "bars": bars,
            "blocked": fl!("devices-activity-blocked", count = tally.blocked),
            "heading": fl!("devices-activity-heading"),
            "overview": fl!("devices-activity-overview"),
            "peak": peak,
            "working": fl!("devices-activity-working", count = tally.working),
        })
    }

    /// Cloud machines still being added: progress only, not lines the keys
    /// reach.
    fn hosted_cloud_jobs(&self) -> Value {
        #[cfg(feature = "cloud")]
        if !self.cloud_jobs.is_empty() {
            let jobs: Vec<Value> = self
                .cloud_jobs
                .iter()
                .map(|job| {
                    json!({
                        "name": job.name,
                        "detail": format!("{} · {}", crate::cloud::name(job.provider), job.status),
                    })
                })
                .collect();
            return json!({ "heading": fl!("devices-adding"), "jobs": jobs });
        }
        Value::Null
    }

    /// Opens the picker from a host's footer, or closes it.
    pub(crate) fn hosted_picker_toggle(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.menu.page == Some(Page::Devices) {
            return self.dismiss_menu(window, cx);
        }
        if self.open_menu(window, cx) {
            self.menu.page = Some(Page::Devices);
            self.menu.selected = Some(0);
        }
    }

    /// Closes the picker, as a click outside it or Escape does.
    pub(crate) fn hosted_picker_dismiss(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.menu.page == Some(Page::Devices) {
            self.dismiss_menu(window, cx);
        }
    }

    /// A key the open picker took, as the window's own picker reads it.
    pub(crate) fn hosted_picker_key(
        &mut self,
        key: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.hosted_picker_open() {
            self.picker_key(key, window, cx);
        }
    }

    /// Does `action` on the selectable line at `ordinal`, if it is still the
    /// line `key` names.
    pub(crate) fn hosted_picker_line(
        &mut self,
        ordinal: usize,
        key: &str,
        action: LineAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.hosted_picker_open() {
            return;
        }
        let lines = self.picker_lines();
        let Some(line) = lines.iter().filter(|line| line.selectable()).nth(ordinal) else {
            return;
        };
        if line_key(self, line) != key {
            return;
        }
        let listing = crate::keymap::secondary(&window.modifiers());
        match (action, line) {
            (LineAction::Choose, Line::Session(Entry::Row { row, .. })) => {
                self.choose_session(row.clone(), window, cx);
            }
            (LineAction::Delete, Line::Session(Entry::Row { row, .. })) => {
                if !matches!(row, Row::Add(_)) {
                    let target = self.management_target(row);
                    self.confirm_session_delete(row.clone(), target, window, cx);
                }
            }
            (LineAction::Sessions, Line::Device(device)) => self.toggle_unfolded(*device, cx),
            (LineAction::Choose, line) => {
                if let Some(index) = self.device_row(line) {
                    if listing {
                        self.toggle_listed_device(index, window, cx);
                    } else {
                        self.choose_device(index, window, cx);
                    }
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests;
