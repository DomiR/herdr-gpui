//! Favorite agents, which the agents panel lists first in any order.
//!
//! Each device's own daemon keeps its agents' favorites, by pane
//! (`sheprd_remote_control::control::favorites`), so every phone and desktop
//! reaching it shows the same stars. [`AgentFavorites`] holds what each
//! reachable daemon last answered, by endpoint ID, and sends a star or unstar
//! at once: it shows at once too, and a change still on its way is laid over
//! each answer until the daemon takes it. A device without a daemon that
//! keeps favorites has none.

mod error;
mod sync;

pub use error::Error;

use crate::device_tags::Daemon;
use gpui::{App, BorrowAppContext, Global};
use sheprd_remote_control::control::favorites::{Change, Favorites};
use std::{
    collections::{BTreeMap, HashMap, VecDeque},
    sync::{Arc, atomic::AtomicBool},
    time::{Duration, Instant},
};

/// How often a reachable machine is asked over SSH. A phone's star reaches
/// this app within it; this app's own go at once.
const REMOTE_INTERVAL: Duration = Duration::from_secs(120);
/// This computer's daemon is a socket away.
const LOCAL_INTERVAL: Duration = Duration::from_secs(15);
/// The most changes waiting for one device's daemon; past it, the oldest go.
const MAX_PENDING: usize = 64;

/// An agent as favorites know it, from a menu or a command.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Starred {
    /// The endpoint ID of the host whose daemon keeps it.
    pub(crate) endpoint: String,
    /// Its pane as that daemon names it (`control::favorites`).
    pub(crate) key: String,
    pub(crate) favorite: bool,
}

/// Every device's favorites as this app shows them, by endpoint ID.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Sets {
    devices: BTreeMap<String, Favorites>,
}

impl Sets {
    /// Whether the agent in `pane` (`control::favorites`' id for it) is a
    /// favorite on the device `device`.
    pub(crate) fn contains(&self, device: &str, pane: &str) -> bool {
        self.devices
            .get(device)
            .is_some_and(|favorites| favorites.contains(pane))
    }

    /// Whether any device has a favorite: until one does, the panel is as
    /// it always was.
    pub(crate) fn any(&self) -> bool {
        self.devices
            .values()
            .any(|favorites| !favorites.panes.is_empty())
    }

    /// `records` as their daemons answered, for tests of what draws them.
    #[cfg(test)]
    pub(crate) fn starred(records: &[(&str, &[&str])]) -> Self {
        let mut sets = Self::default();
        for (device, panes) in records {
            sets.devices.insert(
                (*device).to_owned(),
                Favorites {
                    boot: None,
                    panes: panes.iter().map(|pane| (*pane).to_owned()).collect(),
                },
            );
        }
        sets
    }

    /// Replaces a device's favorites. Returns whether anything changed.
    fn replace(&mut self, device: &str, favorites: Option<Favorites>) -> bool {
        match favorites {
            Some(favorites) if self.devices.get(device) != Some(&favorites) => {
                self.devices.insert(device.to_owned(), favorites);
                true
            }
            Some(_) => false,
            None => self.devices.remove(device).is_some(),
        }
    }
}

/// `stood`, what a daemon answered, with the changes it has not taken yet.
pub(crate) fn with_pending<'a>(
    stood: Favorites,
    pending: impl IntoIterator<Item = &'a Change>,
) -> Favorites {
    pending.into_iter().fold(stood, |favorites, change| {
        favorites
            .clone()
            .changed(change.clone())
            .unwrap_or(favorites)
    })
}

#[derive(Debug, Default)]
struct Link {
    last: Option<Instant>,
    running: bool,
    /// Stars and unstars made here that the daemon has not answered, the
    /// oldest first; the front one is on its way while `running`.
    pending: VecDeque<Change>,
    /// The last exchange failed: what waits goes again after the interval,
    /// not on the next poll, until a new change is made.
    failed: bool,
}

impl Link {
    /// Whether an exchange is due: a change waiting goes at once unless the
    /// last exchange failed, and otherwise the daemon is read once each
    /// `interval`.
    fn due(&self, interval: Duration) -> bool {
        if self.running {
            return false;
        }
        let stale = self.last.is_none_or(|last| last.elapsed() >= interval);
        stale || (!self.failed && !self.pending.is_empty())
    }
}

/// The favorites shown, and each device's link to its daemon.
#[derive(Default)]
pub(crate) struct AgentFavorites {
    sets: Arc<Sets>,
    /// The devices windows can reach now, by endpoint ID.
    daemons: BTreeMap<String, Daemon>,
    links: HashMap<String, Link>,
}

impl Global for AgentFavorites {}

impl AgentFavorites {
    pub(crate) fn install(cx: &mut App) {
        cx.set_global(Self::default());
    }

    /// The favorites to draw from; none before [`Self::install`], as in
    /// tests.
    pub(crate) fn current(cx: &App) -> Arc<Sets> {
        cx.try_global::<Self>()
            .map(|this| this.sets.clone())
            .unwrap_or_default()
    }

    /// Stars or unstars the agent in `pane` on `device`, shows it at once,
    /// and sends it to the device's daemon.
    pub(crate) fn set(cx: &mut App, device: &str, pane: String, favorite: bool) {
        if !cx.has_global::<Self>() {
            return;
        }
        let change = Change { pane, favorite };
        let changed = cx.update_global::<Self, _>(|this, _| {
            let current = this.sets.devices.get(device).cloned().unwrap_or_default();
            let Some(next) = current.changed(change.clone()) else {
                return false;
            };
            let link = this.links.entry(device.to_owned()).or_default();
            if link.pending.len() == MAX_PENDING {
                link.pending.pop_front();
            }
            link.pending.push_back(change);
            // A new change goes at once, even after a failure.
            link.failed = false;
            let mut sets = (*this.sets).clone();
            sets.replace(device, Some(next));
            this.sets = Arc::new(sets);
            true
        });
        if changed {
            cx.refresh_windows();
            Self::tick(cx);
        }
    }

    /// Tells which devices a window can reach. Windows report every poll, so
    /// this only touches the global when the set changed.
    pub(crate) fn reach(cx: &mut App, reachable: &BTreeMap<String, Daemon>) {
        let Some(this) = cx.try_global::<Self>() else {
            return;
        };
        if *reachable != this.daemons {
            cx.global_mut::<Self>().daemons = reachable.clone();
        }
        Self::tick(cx);
    }

    /// Starts the exchanges that are due: a change waiting goes at once, and
    /// otherwise each daemon is read once an interval.
    fn tick(cx: &mut App) {
        let Some(this) = cx.try_global::<Self>() else {
            return;
        };
        let due: Vec<(String, Daemon, Option<Change>)> = this
            .daemons
            .iter()
            .filter_map(|(id, daemon)| {
                let interval = match daemon {
                    Daemon::Local => LOCAL_INTERVAL,
                    Daemon::Remote(_) => REMOTE_INTERVAL,
                };
                let link = this.links.get(id);
                if !link.is_none_or(|link| link.due(interval)) {
                    return None;
                }
                let change = link.and_then(|link| link.pending.front().cloned());
                Some((id.clone(), daemon.clone(), change))
            })
            .collect();
        for (id, daemon, change) in due {
            let link = cx.global_mut::<Self>().links.entry(id.clone()).or_default();
            link.running = true;
            link.last = Some(Instant::now());
            let sent = change.clone();
            let task = cx
                .background_executor()
                .spawn(async move { sync::exchange(&daemon, change, &AtomicBool::new(false)) });
            cx.spawn(async move |cx| {
                let answer = task.await;
                cx.update(|cx| Self::answered(cx, &id, sent.as_ref(), answer));
            })
            .detach();
        }
    }

    fn answered(
        cx: &mut App,
        id: &str,
        sent: Option<&Change>,
        answer: Result<Option<Favorites>, Error>,
    ) {
        if let Err(error) = &answer {
            tracing::debug!(id, %error, "Could not exchange a device's favorite agents");
        }
        let (changed, more) = cx.update_global::<Self, _>(|this, _| {
            let link = this.links.entry(id.to_owned()).or_default();
            link.running = false;
            link.failed = answer.is_err();
            let shown = match answer {
                // A failed exchange tries again after the interval, the
                // changes still waiting.
                Err(_) => return (false, false),
                // Nothing keeps favorites there, so no star can stay.
                Ok(None) => {
                    link.pending.clear();
                    None
                }
                Ok(Some(stood)) => {
                    if sent.is_some() && link.pending.front() == sent {
                        link.pending.pop_front();
                    }
                    Some(with_pending(stood, &link.pending))
                }
            };
            let more = !link.pending.is_empty();
            let mut sets = (*this.sets).clone();
            let changed = sets.replace(id, shown);
            if changed {
                this.sets = Arc::new(sets);
            }
            (changed, more)
        });
        if changed {
            cx.refresh_windows();
        }
        if more {
            Self::tick(cx);
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests;
