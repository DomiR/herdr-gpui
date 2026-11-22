//! The fleet, Sheprd's side (`docs/relay-plan.md`, Fleet): this computer's
//! daemon keeps the signed log and acts on it; Sheprd shows it in
//! Settings > Fleet, asks the daemon for what a person here approved, and
//! carries the log over SSH to every machine it reaches that is in the
//! same fleet.
//!
//! - [`remote`]: the halves run on another machine over SSH.
//! - [`flows`]: syncing a machine, inviting a computer, sharing a host.
//! - [`saved`]: each fleet host this computer may reach, kept as a saved
//!   device, `USER@r-ID.sheprd.invalid`.
//! - [`plain`] and [`agent`]: plain hosts, which run no Sheprd daemon:
//!   installing their relay agent and keeping its access file current.
//!
//! Syncing never hands this fleet's log to a machine outside it: a machine
//! is first asked, with a nonce and without naming the fleet, for what it
//! holds, and only one that answers for this fleet and signs the nonce
//! with the certificate authority of one of the fleet's computers or
//! Sheprd hosts goes on; any other is left alone.

pub(crate) mod agent;
pub(crate) mod bluetooth;
mod error;
pub(crate) mod flows;
pub(crate) mod joins;
pub(crate) mod plain;
mod remote;
pub(crate) mod saved;

pub use error::Error;

use crate::device_tags::Daemon;
use crate::remote::Supervisor;
use gpui::{App, BorrowAppContext, EntityId, Global};
use sheprd_client::ConnectTarget;
use sheprd_remote_control::control::{
    Request, Response,
    fleet::{
        View,
        sync::{Answer, Ask},
    },
};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    time::{Duration, Instant},
};

/// How often a machine in the fleet is synced when nothing changed.
const REFRESH: Duration = Duration::from_secs(600);
/// How soon a machine that is in no fleet, or another, is asked again.
const OUTSIDE: Duration = Duration::from_secs(3600);
/// How soon a machine whose sync failed is tried again.
const RETRY: Duration = Duration::from_secs(120);
/// How often the view is read again while nothing asks for it.
const VIEW: Duration = Duration::from_secs(60);
/// How long a window's report counts.
const WINDOW_SILENCE: Duration = Duration::from_secs(30);

/// A machine some window reaches: its target and the name it shows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Machine {
    pub(crate) target: ConnectTarget,
    pub(crate) label: String,
}

/// Where one machine stands.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Link {
    /// Its heads when it last answered, so the next sync carries only what
    /// it lacks.
    heads: Vec<String>,
    /// This computer's heads when it last synced the machine.
    synced: Vec<String>,
    last: Option<Instant>,
    running: bool,
    /// The host it last proved to be.
    host: Option<String>,
    pub(crate) standing: Standing,
}

/// What a machine said about the fleet.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Standing {
    #[default]
    Unknown,
    /// In this fleet.
    Member,
    /// In no fleet or in another.
    Outside,
    /// Runs no Sheprd daemon, nor a relay agent of this fleet: it may be
    /// shared as a plain host.
    NoDaemon,
    /// A plain host of this fleet, whose relay agent is kept current.
    Agent,
    /// Its SSH host key is not checked: the fleet neither shares it as a
    /// plain host nor asks its agent anything.
    Unchecked,
    /// Could not be asked.
    Failed,
    /// A host the fleet removed that holds its removal, or forgot the
    /// fleet: not synced again.
    Removed,
}

impl Link {
    /// Keeps what a sync found: the machine's `standing`, its `heads` and
    /// this computer's `synced` heads, and the host it proved to be, or
    /// whose agent it runs, when it said. A host is never forgotten for a
    /// sync that could not tell, so a removed host the fleet shares again
    /// is recognized ([`Fleet::forget_stale_links`]).
    pub(super) fn settle(
        &mut self,
        standing: Standing,
        heads: Vec<String>,
        synced: Vec<String>,
        host: Option<String>,
    ) {
        self.running = false;
        self.standing = standing;
        self.heads = heads;
        self.synced = synced;
        if host.is_some() {
            self.host = host;
        }
    }

    /// Starts over, as for a machine never asked; a sync running for it
    /// still finishes.
    pub(super) fn restart(&mut self) {
        *self = Self {
            running: self.running,
            ..Self::default()
        };
    }

    fn due(&self, mine: &[String], now: Instant) -> bool {
        if self.running {
            return false;
        }
        let since = |wait| {
            self.last
                .is_none_or(|last| now.duration_since(last) >= wait)
        };
        match self.standing {
            Standing::Unknown => true,
            Standing::Member | Standing::Agent => self.synced != mine || since(REFRESH),
            Standing::Outside | Standing::NoDaemon | Standing::Unchecked => since(OUTSIDE),
            Standing::Failed => since(RETRY),
            Standing::Removed => false,
        }
    }
}

#[derive(Debug)]
struct Reached {
    machines: BTreeMap<String, Machine>,
    at: Instant,
}

/// The fleet as Sheprd last read it, and each machine's link.
#[derive(Default)]
pub(crate) struct Fleet {
    view: Option<View>,
    /// The daemon answered: `view` is `None` because this computer is in
    /// no fleet, not because nothing was asked yet.
    read: bool,
    asked: Option<Instant>,
    asking: bool,
    /// How many reads were asked for: only the newest one's answer is
    /// taken, so an older answer arriving late never replaces a newer view.
    reads: u64,
    /// The daemon answered a read, saying this computer's fleet or none.
    answered: bool,
    /// Orders the saved devices' updates ([`saved::keep`]).
    saving: saved::Saving,
    /// Fleet hosts the last update could not save as devices.
    unsaved: Option<saved::Unsaved>,
    error: Option<String>,
    /// Why the fleet's part of a Bluetooth pairing failed: kept apart from
    /// `error`, which the next good read clears, until a pairing's fleet
    /// step succeeds.
    pairing_error: Option<String>,
    windows: HashMap<EntityId, Reached>,
    links: HashMap<String, Link>,
    /// Machines an invitation or share runs for.
    acting: BTreeSet<String>,
    /// Phones approved at pairing with Also add to your fleet, by when.
    expected: joins::Expected,
    /// Joins waiting for a person.
    joins: Vec<sheprd_remote_control::control::fleet::sync::Join>,
}

impl Global for Fleet {}

impl Fleet {
    pub(crate) fn install(cx: &mut App) {
        cx.set_global(Self::default());
    }

    /// The fleet as this computer's daemon last said.
    pub(crate) fn view(cx: &App) -> Option<&View> {
        cx.try_global::<Self>()?.view.as_ref()
    }

    /// This computer's fleet, while it is a member.
    pub(crate) fn member_of(cx: &App) -> Option<String> {
        cx.try_global::<Self>()?.member_fleet().map(str::to_owned)
    }

    fn member_fleet(&self) -> Option<&str> {
        let view = self.view.as_ref()?;
        view.members
            .iter()
            .any(|member| member.id == view.me)
            .then_some(view.fleet.as_str())
    }

    /// Whether the daemon has answered at least once.
    pub(crate) fn read(cx: &App) -> bool {
        cx.try_global::<Self>().is_some_and(|fleet| fleet.read)
    }

    /// Fleet hosts that could not be saved as devices, if any.
    pub(crate) fn unsaved(cx: &App) -> Option<saved::Unsaved> {
        cx.try_global::<Self>()?.unsaved.clone()
    }

    /// Records which fleet hosts could not be saved as devices.
    pub(crate) fn set_unsaved(cx: &mut App, unsaved: Option<saved::Unsaved>) {
        cx.update_global::<Self, _>(|fleet, _| fleet.unsaved = unsaved);
        cx.refresh_windows();
    }

    /// The last failure to show, if any.
    pub(crate) fn error(cx: &App) -> Option<String> {
        let fleet = cx.try_global::<Self>()?;
        fleet.error.clone().or_else(|| fleet.pairing_error.clone())
    }

    /// Every machine a window reaches now, by id.
    pub(crate) fn machines(cx: &App) -> BTreeMap<String, Machine> {
        cx.try_global::<Self>()
            .map(|fleet| fleet.reached(Instant::now()))
            .unwrap_or_default()
    }

    /// What `machine` said about the fleet.
    pub(crate) fn standing(cx: &App, machine: &str) -> Standing {
        cx.try_global::<Self>()
            .and_then(|fleet| fleet.links.get(machine))
            .map_or(Standing::Unknown, |link| link.standing)
    }

    /// Whether an invitation or share runs for `machine`.
    pub(crate) fn acting(cx: &App, machine: &str) -> bool {
        cx.try_global::<Self>()
            .is_some_and(|fleet| fleet.acting.contains(machine))
    }

    /// Tells which devices `window` reaches, as `relay_sharing` hears it;
    /// this computer's own daemon is asked directly, not over SSH.
    pub(crate) fn reach(cx: &mut App, window: EntityId, reachable: &BTreeMap<String, Daemon>) {
        if !cx.has_global::<Self>() {
            return;
        }
        let machines = reachable
            .iter()
            .filter_map(|(id, daemon)| match daemon {
                Daemon::Local => None,
                Daemon::Remote(target) => Some((
                    id.clone(),
                    Machine {
                        target: target.clone(),
                        label: label(id, target),
                    },
                )),
            })
            .collect();
        let now = Instant::now();
        cx.update_global::<Self, _>(|this, _| {
            this.windows.insert(window, Reached { machines, at: now });
            this.windows
                .retain(|_, reached| now.duration_since(reached.at) < WINDOW_SILENCE);
            let reached = this.reached(now);
            this.links
                .retain(|id, link| link.running || reached.contains_key(id));
        });
        Self::tick(cx);
    }

    fn reached(&self, now: Instant) -> BTreeMap<String, Machine> {
        self.windows
            .values()
            .filter(|reached| now.duration_since(reached.at) < WINDOW_SILENCE)
            .flat_map(|reached| reached.machines.clone())
            .collect()
    }

    /// Reads the view when it is stale, then syncs the machines that are
    /// due.
    pub(crate) fn tick(cx: &mut App) {
        let Some(this) = cx.try_global::<Self>() else {
            return;
        };
        let now = Instant::now();
        if !this.asking
            && this
                .asked
                .is_none_or(|asked| now.duration_since(asked) >= this.pace())
        {
            Self::refresh(cx);
        }
        let Some(this) = cx.try_global::<Self>() else {
            return;
        };
        let Some(view) = &this.view else {
            return;
        };
        let mine = view.heads.clone();
        let fleet = view.fleet.clone();
        let due: Vec<(String, Machine)> = this
            .reached(now)
            .into_iter()
            .filter(|(id, _)| {
                !this.acting.contains(id)
                    && this.links.get(id).is_none_or(|link| link.due(&mine, now))
            })
            .collect();
        for (id, machine) in due {
            flows::sync(cx, id, machine, fleet.clone());
        }
    }

    /// Asks the daemon for the view now.
    pub(crate) fn refresh(cx: &mut App) {
        let Some(client) = Supervisor::client(cx) else {
            return;
        };
        let read = cx.update_global::<Self, _>(|this, _| {
            this.asking = true;
            this.asked = Some(Instant::now());
            this.reads = this.reads.wrapping_add(1);
            this.reads
        });
        let task = client.request(Request::Fleet(Box::new(Ask::Status)));
        cx.spawn(async move |cx| {
            let answer = task.await;
            cx.update(|cx| {
                let view = match answer {
                    Ok(Response::Fleet(answer)) => match *answer {
                        Answer::Status { view } => Ok(view.map(|view| *view)),
                        _ => Err(None),
                    },
                    Ok(Response::Error { message, .. }) => Err(Some(message)),
                    Ok(_) => Err(None),
                    Err(error) => Err(Some(error.to_string())),
                };
                let member = matches!(&view, Ok(Some(view)) if view.members.iter().any(|member| member.id == view.me));
                let changed = cx.update_global::<Self, _>(|this, _| this.answered(read, view));
                if member {
                    Self::read_joins(cx);
                }
                if changed {
                    saved::keep(cx);
                    Self::tick(cx);
                }
                cx.refresh_windows();
            });
        })
        .detach();
    }

    /// Takes the answer to the `read`th read: whether the view changed. An
    /// answer to an older read is dropped, as a newer one is on its way.
    fn answered(&mut self, read: u64, view: Result<Option<View>, Option<String>>) -> bool {
        if read != self.reads {
            return false;
        }
        self.asking = false;
        self.read = true;
        match view {
            // The first answer counts as a change even when it says no
            // fleet: devices of a fleet left while Sheprd was closed go.
            Ok(view) => {
                // The daemon answered: an earlier failure no longer holds.
                self.error = None;
                let changed = !self.answered || self.view != view;
                self.answered = true;
                self.forget_stale_links(view.as_ref());
                self.view = view;
                changed
            }
            // An older daemon, or none: the page says so.
            Err(message) => {
                self.error = message;
                false
            }
        }
    }

    /// Forgets what machines said when it no longer holds under `view`:
    /// every link once this computer is in another fleet or none, and the
    /// stop on a removed host the fleet shares again.
    fn forget_stale_links(&mut self, view: Option<&View>) {
        let fleet = |view: Option<&View>| view.map(|view| view.fleet.clone());
        let other = fleet(self.view.as_ref()) != fleet(view);
        for link in self.links.values_mut() {
            let shared_again = link.standing == Standing::Removed
                && view.is_some_and(|view| {
                    view.hosts
                        .iter()
                        .any(|host| Some(&host.id) == link.host.as_ref())
                });
            if other || shared_again {
                link.restart();
            }
        }
    }

    /// What became of the fleet's part of a Bluetooth pairing: a failure
    /// stays in Settings > Fleet until a later one succeeds, whatever the
    /// daemon's reads say meanwhile.
    pub(crate) fn pairing_done(cx: &mut App, failure: Option<String>) {
        if cx.has_global::<Self>() {
            cx.update_global::<Self, _>(|this, _| this.pairing_error = failure);
        }
    }

    /// Asks the daemon `ask`, which a person here approved, then reads the
    /// view again. `done` gets a failure to show.
    pub(crate) fn act(
        cx: &mut App,
        ask: Ask,
        done: impl FnOnce(Option<String>, &mut App) + 'static,
    ) {
        Self::act_all(cx, vec![ask], done);
    }

    /// Asks the daemon each of `asks` in order, which a person here
    /// approved as one change, stopping at the first failure, then reads
    /// the view again. `done` gets a failure to show.
    pub(crate) fn act_all(
        cx: &mut App,
        asks: Vec<Ask>,
        done: impl FnOnce(Option<String>, &mut App) + 'static,
    ) {
        let Some(client) = Supervisor::client(cx) else {
            done(Some(Error::NoDaemon.to_string()), cx);
            return;
        };
        cx.spawn(async move |cx| {
            let mut failure = None;
            for ask in asks {
                failure = match client.request(Request::Fleet(Box::new(ask))).await {
                    Ok(Response::Fleet(_)) => None,
                    Ok(Response::Error { message, .. }) => Some(message),
                    Ok(_) => Some(Error::Daemon(String::new()).to_string()),
                    Err(error) => Some(error.to_string()),
                };
                if failure.is_some() {
                    break;
                }
            }
            cx.update(|cx| {
                Self::refresh(cx);
                done(failure, cx);
            });
        })
        .detach();
    }

    /// Sets what `machine` said, as a sync that just ran would, so it is
    /// not due again.
    #[cfg(test)]
    pub(crate) fn set_standing(cx: &mut App, machine: &str, standing: Standing) {
        let heads = Self::view(cx)
            .map(|view| view.heads.clone())
            .unwrap_or_default();
        Self::set_link(cx, machine, |link| {
            link.standing = standing;
            link.synced = heads;
            link.last = Some(Instant::now());
        });
    }

    fn set_link(cx: &mut App, id: &str, change: impl FnOnce(&mut Link)) {
        cx.update_global::<Self, _>(|this, _| change(this.links.entry(id.to_owned()).or_default()));
    }

    fn set_acting(cx: &mut App, id: &str, acting: bool) {
        cx.update_global::<Self, _>(|this, _| {
            if acting {
                this.acting.insert(id.to_owned());
            } else {
                this.acting.remove(id);
            }
        });
        cx.refresh_windows();
    }
}

/// The name a machine shows: its SSH target, or a cloud machine's name.
fn label(id: &str, target: &ConnectTarget) -> String {
    match target {
        ConnectTarget::Ssh { target, .. } => target.clone(),
        #[cfg(feature = "cloud")]
        ConnectTarget::Cloud { machine, .. } => machine.clone(),
        ConnectTarget::Wsl { distro, .. } => distro.clone(),
        _ => id.to_owned(),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests;
