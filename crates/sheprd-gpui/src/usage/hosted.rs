//! The status bar's usage as data, for a layout that draws the bar itself:
//! the segments [`SheprdWindow::render_usage`] draws, with the same
//! providers, meter, labels and colors, and nothing when it draws nothing.

use super::{
    Reading, registry,
    render::{bar_windows, color},
    text,
};
use crate::{
    config::status_bar::Detail,
    i18n::{Localize, fl},
    sidebar::hosted::hex,
    window::SheprdWindow,
};
use serde_json::{Value, json};
use std::time::{Instant, SystemTime};

impl SheprdWindow {
    /// What the status bar's usage shows: `segments`, one per provider, and
    /// `error` when the host could not be read and there is nothing older;
    /// `null` while usage is hidden or there is nothing to show.
    pub(crate) fn hosted_usage(&self) -> Value {
        let entry = self.usage.current();
        let shown = entry
            .map(|entry| entry.headline(self.usage.chosen()))
            .unwrap_or_default();
        let error = entry
            .and_then(|entry| entry.error.as_deref())
            .filter(|_| shown.is_empty())
            .map(Localize::localize);
        if !self.config.usage.show || (shown.is_empty() && error.is_none()) {
            return Value::Null;
        }
        let now = SystemTime::now();
        let segments: Vec<Value> = shown
            .iter()
            .map(|reading| self.usage_segment_state(reading, now))
            .collect();
        json!({
            "busy": self.usage.busy(),
            "error": error,
            // The bar's own words, in the app's language.
            "refresh_label": fl!("usage-refresh-usage"),
            "segments": segments,
            "unavailable": fl!("usage-unavailable"),
        })
    }

    /// One provider's segment: its id and name, then, once it has numbers,
    /// the tightest window's meter and the labels the configured detail
    /// shows, each in its severity's color.
    fn usage_segment_state(&self, reading: &Reading, now: SystemTime) -> Value {
        let theme = &self.theme;
        let provider = reading.provider;
        let mut segment = json!({
            "id": provider.id(),
            "name": provider.name(),
            "locked": reading.report.is_none() && reading.access.is_some(),
            "stale": reading.error.is_some(),
            "stale_color": hex(theme.ink(theme.palette[3])),
        });
        let Some(report) = &reading.report else {
            return segment;
        };
        let label = |text: String, used: f32| json!({ "text": text, "color": hex(color(used.into(), theme, theme.foreground)) });
        let labels: Vec<Value> = match self.config.status_bar.usage {
            Detail::Compact => report
                .tightest()
                .map(|tightest| label(format!("{}%", tightest.percent()), tightest.used))
                .into_iter()
                .collect(),
            Detail::Detailed => {
                if let Some(tightest) = report.tightest() {
                    segment["meter"] = json!({
                        "used": tightest.used / 100.,
                        "color": hex(color(tightest.used.into(), theme, theme.muted)),
                        "track": hex(theme.active),
                    });
                }
                bar_windows(&report.windows)
                    .map(|window| label(text::bar_label(window, now), window.used))
                    .collect()
            }
        };
        segment["labels"] = labels.into();
        // A service that meters money or credits rather than a window shows
        // what is left or spent.
        if report.windows.is_empty()
            && let Some(balance) = report.balances.first()
        {
            segment["balance"] = balance.amount_text().into();
        }
        segment
    }

    /// Opens `id`'s usage panel above the status bar at `x`, as a click on
    /// its segment does; an id no longer registered is ignored.
    pub(crate) fn open_hosted_usage(
        &mut self,
        id: &str,
        x: gpui::Pixels,
        window: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(provider) = registry::find(id) else {
            return;
        };
        let top = window.viewport_size().height - gpui::px(self.status_bar_height());
        let anchor = gpui::point(x, top - gpui::px(super::panel::PANEL_GAP));
        self.open_usage(provider, anchor, window, cx);
    }

    /// Reads usage again now, as the bar's refresh button does.
    pub(crate) fn refresh_hosted_usage(&mut self, cx: &mut gpui::Context<Self>) {
        self.usage.refresh(Instant::now());
        cx.notify();
    }
}
