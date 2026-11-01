//! The banners above a section's list: a read-only database, a worker's
//! failure, and each source's failed or capped sync. Clicking one syncs
//! again; its close button hides it until it says something else, or until
//! its trouble clears and comes back ([`Dismissed`]).

use super::{
    OrchestratorView,
    look::{Look, RED, YELLOW},
    repo::Repo,
};
use crate::{
    config::corners,
    orchestrator::{Access, SourceKey, SyncState},
};
use gpui::{prelude::*, *};
use std::collections::HashMap;

/// Where a banner shows: about one project's database, worker, or source.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(super) struct Slot {
    repo: String,
    of: Of,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum Of {
    ReadOnly,
    Error,
    Source(SourceKey),
}

/// The banners closed, by slot, with what each said then.
#[derive(Debug, Default)]
pub(super) struct Dismissed(HashMap<Slot, String>);

impl Dismissed {
    fn hides(&self, slot: &Slot, text: &str) -> bool {
        self.0.get(slot).is_some_and(|closed| closed == text)
    }

    /// Forgets a closed banner whose trouble has cleared: the source synced,
    /// the error went, the project left. A source syncing again after the
    /// failure is still in the same trouble.
    pub(super) fn settle(&mut self, repos: &[Repo]) {
        self.0.retain(|slot, _| {
            let Some(repo) = repos.iter().find(|repo| repo.key == slot.repo) else {
                return false;
            };
            let snapshot = &repo.snapshot;
            match &slot.of {
                Of::ReadOnly => matches!(snapshot.access, Some(Access::ReadOnly { .. })),
                Of::Error => snapshot.error.is_some(),
                Of::Source(key) => snapshot.sources.iter().any(|status| {
                    status.key == *key
                        && matches!(
                            status.state,
                            SyncState::Failed(_)
                                | SyncState::Capped { .. }
                                | SyncState::Syncing { .. }
                        )
                }),
            }
        });
    }
}

impl OrchestratorView {
    pub(super) fn render_banners(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let look = &self.look;
        let mut banners = Vec::new();
        let named = self.projects_named().count() > 1;
        for repo in self.projects_named() {
            // With several projects listed, each says whose trouble it is.
            let of = |text: String| {
                if named {
                    format!("{}: {text}", repo.name())
                } else {
                    text
                }
            };
            let slot = |of: Of| Slot {
                repo: repo.key.clone(),
                of,
            };
            let snapshot = &repo.snapshot;
            if let Some(Access::ReadOnly { found }) = snapshot.access {
                banners.push((
                    slot(Of::ReadOnly),
                    RED,
                    "icons/lock.svg",
                    of(format!(
                        "Shared data is schema {found}; this herdr-gpui understands up to {}. Read-only until updated.",
                        crate::orchestrator::SCHEMA_VERSION
                    )),
                ));
            }
            if let Some(error) = &snapshot.error {
                banners.push((slot(Of::Error), RED, "icons/x.svg", of(error.to_string())));
            }
            for status in &snapshot.sources {
                let text = match &status.state {
                    SyncState::Failed(error) => {
                        format!("{}: {error}", status.key.provider.name())
                    }
                    SyncState::Capped { count } => format!(
                        "{} has more than {count} items; the newest are shown and not cached.",
                        status.key.provider.name()
                    ),
                    _ => continue,
                };
                let hue = if matches!(status.state, SyncState::Capped { .. }) {
                    YELLOW
                } else {
                    RED
                };
                banners.push((
                    slot(Of::Source(status.key.clone())),
                    hue,
                    "icons/refresh.svg",
                    of(text),
                ));
            }
        }
        banners
            .into_iter()
            .filter(|(slot, _, _, text)| !self.dismissed.hides(slot, text))
            .enumerate()
            .map(|(index, (slot, hue, icon, text))| {
                let close = look
                    .icon_button(
                        ("orchestrator-banner-close", index),
                        "icons/close.svg",
                        "Dismiss",
                    )
                    .debug_selector(|| "orchestrator-banner-close".into())
                    .on_click(cx.listener({
                        let text = text.clone();
                        move |this, _, _, cx| {
                            // The banner itself syncs again; closing it must not.
                            cx.stop_propagation();
                            this.dismissed.0.insert(slot.clone(), text.clone());
                            cx.notify();
                        }
                    }));
                banner(look, hue, icon, text)
                    .child(close)
                    .debug_selector(|| "orchestrator-banner".into())
                    .id(("orchestrator-banner", index))
                    .cursor_pointer()
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.ask_refresh();
                        cx.notify();
                    }))
                    .into_any_element()
            })
            .collect()
    }
}

fn banner(look: &Look, hue: usize, icon: &'static str, text: String) -> Div {
    div()
        .flex_none()
        .mx_4()
        .mb_2()
        .flex()
        .items_center()
        .gap_2()
        .px_3()
        .py_2()
        .rounded(px(corners::CONTROL))
        .bg(rgb(look.wash(hue)))
        .child(look.icon(icon, 13., look.hue(hue)))
        .child(div().flex_1().min_w_0().text_size(look.small()).child(text))
}
