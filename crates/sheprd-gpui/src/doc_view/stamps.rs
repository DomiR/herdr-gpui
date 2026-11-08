//! Local times used by Markdown task stamps.

use chrono::{Local, NaiveDateTime};
use gpui::App;

/// The local time a task's state stamp records, or `None` when stamps are off.
pub(crate) fn stamp_time(cx: &App) -> Option<NaiveDateTime> {
    cx.try_global::<crate::app::InitialAppearance>()
        .is_none_or(|appearance| appearance.config.documents.stamps)
        .then(now)
}

/// The local time now, for stamps written on request (`doc todo start`).
pub(crate) fn now() -> NaiveDateTime {
    Local::now().naive_local()
}
