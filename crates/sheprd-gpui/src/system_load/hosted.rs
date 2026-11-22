//! The status bar's load as data, for a layout that draws the bar itself:
//! what [`SheprdWindow::render_system_load`] draws, with the same sparkline,
//! meter, shares and colors, and nothing when it draws nothing.

use super::{
    HISTORY,
    render::{CPU_WARN, MEMORY_WARN, severity},
};
use crate::{config::status_bar::Detail, i18n::fl, sidebar::hosted::hex, window::SheprdWindow};
use serde_json::{Value, json};

impl SheprdWindow {
    /// The selected host's load: `cpu` and `memory` as percentages, their
    /// `cpu_color` and `memory_color`, and while the bar is detailed the
    /// CPU `history` (oldest first, [`HISTORY`] bars, `false` for those not
    /// sampled yet) and the memory `meter`. `null` while hidden or before
    /// the host's first answer; `unavailable` when it never answered.
    pub(crate) fn hosted_load(&self) -> Value {
        if !self.config.show_system_load {
            return Value::Null;
        }
        let Some(reading) = self
            .selected_host()
            .and_then(|host| self.system_load.get(&host))
        else {
            return Value::Null;
        };
        let theme = &self.theme;
        let Some(sample) = reading.latest() else {
            return match reading.error() {
                Some(_) => json!({ "unavailable": fl!("system-load-unavailable") }),
                None => Value::Null,
            };
        };
        // The numbers are the last good ones while a refresh fails.
        let stale = reading.error().is_some();
        let share = |value: f32, warn: f32| {
            hex(if stale {
                theme.muted
            } else {
                severity(value, warn, theme, theme.foreground)
            })
        };
        let memory = sample.memory.map(|memory| memory.percent());
        let mut load = json!({
            "cpu": sample.cpu,
            "cpu_color": sample.cpu.map(|cpu| share(cpu, CPU_WARN)),
            "cpu_label": fl!("system-load-cpu"),
            "label_color": hex(theme.muted),
            "memory": memory,
            "memory_color": memory.map(|used| share(used, MEMORY_WARN)),
            "memory_label": fl!("system-load-memory"),
        });
        if self.config.status_bar.system_load == Detail::Detailed {
            let empty = HISTORY.saturating_sub(reading.history().len());
            let bars: Vec<Value> = std::iter::repeat_n(Value::Bool(false), empty)
                .chain(reading.history().map(|value| {
                    json!({
                        "value": value / 100.,
                        "color": hex(severity(value, CPU_WARN, theme, theme.muted)),
                    })
                }))
                .collect();
            load["history"] = bars.into();
            load["track"] = hex(theme.active).into();
            if let Some(used) = memory {
                load["meter"] = json!({
                    "used": used / 100.,
                    "color": hex(severity(used, MEMORY_WARN, theme, theme.muted)),
                });
            }
        }
        load
    }
}
