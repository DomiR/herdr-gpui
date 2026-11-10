//! The banners above a section's list: a read-only database, a worker's
//! failure, and each source's failed or capped sync. Clicking one syncs
//! again; its close button hides it until it says something else, or until
//! its trouble clears and comes back ([`Dismissed`]).

use super::{
    Event, OrchestratorView,
    inbox::failure_text,
    look::{Look, RED, YELLOW},
    repo::Repo,
};
use crate::{
    config::corners,
    i18n::{Localize, fl},
    orchestrator::{Access, Provider, SourceKey, SyncState},
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
                    fl!("orchestrator-banner-of", project = repo.name(), text = text)
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
                    of(fl!(
                        "orchestrator-newer-schema",
                        found = found,
                        known = crate::orchestrator::SCHEMA_VERSION
                    )),
                    None,
                ));
            }
            if let Some(error) = &snapshot.error {
                banners.push((
                    slot(Of::Error),
                    RED,
                    "icons/x.svg",
                    of(error.localize()),
                    None,
                ));
            }
            // Each section reports only its own sources' trouble, as its
            // header shows only their chips: GitHub's failures are not the
            // Linear board's, nor Linear's the projects'.
            let sources = snapshot
                .sources
                .iter()
                .filter(|status| self.issue_source.lists(status.key.provider));
            for status in sources {
                // Azure DevOps has no sign-in of its own in the title bar
                // for most users, so its missing token says where to add one.
                if status.key.provider == Provider::Azure
                    && matches!(status.state, SyncState::SignIn)
                {
                    let text = fl!(
                        "azure-devops-menu-connect-banner",
                        repository = status.key.repository.as_str()
                    );
                    banners.push((
                        slot(Of::Source(status.key.clone())),
                        YELLOW,
                        "icons/azure-devops.svg",
                        of(text),
                        Some(Provider::Azure),
                    ));
                    continue;
                }
                let text = match &status.state {
                    SyncState::Failed(error) => failure_text(status.key.provider, error),
                    SyncState::Capped { count } => fl!(
                        "orchestrator-capped",
                        source = status.key.provider.name(),
                        count = *count
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
                    None,
                ));
            }
        }
        banners
            .into_iter()
            .filter(|(slot, _, _, text, _)| !self.dismissed.hides(slot, text))
            .enumerate()
            .map(|(index, (slot, hue, icon, text, sign_in))| {
                let close = look
                    .icon_button(
                        ("orchestrator-banner-close", index),
                        "icons/close.svg",
                        fl!("orchestrator-dismiss"),
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
                    .debug_selector(move || match sign_in {
                        Some(_) => "orchestrator-banner-sign-in".into(),
                        None => "orchestrator-banner".into(),
                    })
                    .id(("orchestrator-banner", index))
                    .cursor_pointer()
                    .on_click(cx.listener(move |this, _, _, cx| {
                        match sign_in {
                            Some(provider) => cx.emit(Event::SignIn(provider)),
                            None => this.ask_refresh(),
                        }
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
