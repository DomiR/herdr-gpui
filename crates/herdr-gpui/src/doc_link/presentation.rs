//! Prepared link state rendered by a Markdown editor's toolbar.
// i18n: migrated

use super::{Mode, links};
use crate::i18n::{Localize, fl};
use gpui::App;
use std::path::Path;

pub(crate) struct Presentation {
    pub(crate) enabled: bool,
    pub(crate) label: String,
    pub(crate) on: bool,
    pub(crate) tooltip: String,
}

pub(crate) fn label(mode: Mode) -> String {
    match mode {
        Mode::Off => fl!("doc-link-connect"),
        Mode::On => fl!("doc-link-connected"),
        Mode::Paused => fl!("doc-link-paused"),
        Mode::Restoring => fl!("doc-link-restoring"),
        Mode::Starting => fl!("doc-link-connecting"),
    }
}

pub(crate) fn presentation(path: &Path, supported: bool, loaded: bool, cx: &App) -> Presentation {
    let entity = links(cx);
    let links = entity.as_ref().map(|entity| entity.read(cx));
    let status = links.and_then(|links| links.status(path));
    let mode = status.as_ref().map_or(Mode::Off, |status| status.mode);
    let on = mode != Mode::Off;
    let error = links.and_then(|links| links.error(path));
    let tooltip = if !supported && !on {
        fl!("error-doc-link-unsupported")
    } else if let Some(error) = error {
        error.localize()
    } else if mode == Mode::Paused && links.is_some_and(|links| links.not_ready(path)) {
        fl!("doc-link-waiting")
    } else if mode == Mode::Starting
        && status
            .as_ref()
            .is_none_or(|status| status.agent_name.is_none())
    {
        fl!("doc-link-starting-help")
    } else if on {
        fl!(
            "doc-link-on-help",
            agent = status
                .and_then(|status| status.agent_name)
                .unwrap_or_default()
        )
    } else {
        fl!("doc-link-off-help")
    };
    Presentation {
        enabled: entity.is_some() && (on || (supported && loaded)),
        label: label(mode),
        on,
        tooltip,
    }
}
