//! The orchestrator's worker: one thread per open view, owning the shared
//! database connection and every host script and network call. The UI sends
//! bounded commands and reads the latest [`Snapshot`] from a coalescing
//! mailbox on its tick; it never waits on this thread.

use super::{
    Access, Error, HerdrSession, Item, Provider, Result, Run, SourceKey, Store,
    actions::{Action, Site},
    beads, database_path, github, gitlab,
    location::{database_path_in, legacy_path, legacy_path_in},
    naming, prompt,
    repo::{self, Extras, RepoInfo},
    ssh_slots,
};
use crate::cache::{Cache, Kind};
use herdr_client::ConnectTarget;
use herdr_github::Tokens;
use herdr_remote::teleport::{AgentKind, Host};
use linear::LinearKey;
use secrecy::SecretString;
use serde_json::Value;
use sources::{recognize, statuses};
use std::{
    collections::{HashMap, VecDeque},
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc::{self, RecvTimeoutError, SyncSender, TrySendError},
    },
    time::{Duration, Instant},
};

mod gitlab_source;
mod retry;
mod snapshot;
mod sources;
mod sync;

use retry::Retry;
pub use retry::retry_wait;
pub use snapshot::{Notice, PullRequestLookup, Snapshot, SourceStatus, SyncState};
pub use sources::sources;
use sync::Linear;

/// How often other applications' writes (agent-launcher's runs) are looked for.
const RELOAD: Duration = Duration::from_secs(5);
/// How often sources sync, as agent-launcher's default poll interval.
const SYNC: Duration = Duration::from_secs(60);
/// How often Linear syncs. A page of its issues costs about 4,650 of the
/// 3,000,000 complexity points Linear allows a user an hour, so a listing
/// of 500 each minute would use them all.
const LINEAR_SYNC: Duration = Duration::from_secs(300);

/// The worker's periods; tests shorten them.
#[derive(Clone, Copy, Debug)]
pub struct Timing {
    pub reload: Duration,
    pub sync: Duration,
    /// Linear's own sync interval, checked on each sync.
    pub linear: Duration,
}

impl Default for Timing {
    fn default() -> Self {
        Self {
            reload: RELOAD,
            sync: SYNC,
            linear: LINEAR_SYNC,
        }
    }
}
const COMMANDS: usize = 8;

/// Actions running at once; more wait for the user to try again.
const MAX_ACTIONS: usize = 4;
/// Outcomes kept for the view between its polls.
const MAX_NOTICES: usize = 8;

/// Where the view's repository is and whom to read GitHub as.
#[derive(Clone)]
pub struct Request {
    pub target: ConnectTarget,
    /// Any checkout of the repository on `target`.
    pub checkout: String,
    /// The Herdr workspace the view was opened from, whose repository new
    /// worktrees are created in.
    pub workspace_id: String,
    /// The accounts GitHub is read with; each source uses the one that
    /// reads its repository's owner.
    pub token: Option<Tokens>,
    /// Where GitLab tokens pasted in the account menu are saved.
    pub gitlab_store: herdr_credentials::Store,
    /// The folder holding the databases, and agent-launcher's to carry
    /// over, in place of the platform's; tests set it.
    pub data_root: Option<PathBuf>,
    pub timing: Timing,
    /// Where the Linear API key is saved; `None` leaves Linear out entirely.
    pub linear: Option<herdr_credentials::Store>,
}

enum Command {
    /// Sync every source now, reading GitHub as `token`.
    Refresh(Option<Tokens>),
    /// Do what the user asked, on a thread of its own.
    Act(Action),
    /// An action finished: read the database again, and sync when it changed
    /// a source.
    Acted { resync: bool },
    /// Look up a pull request's status, checks, and merge state by number.
    LoadPullRequest(u64),
    LoadAgents {
        endpoint: Option<String>,
        target: ConnectTarget,
    },
    /// Sync Linear now: its section shows again.
    SyncLinear,
    /// Save the Linear API key, or with `None` remove it, then sync Linear.
    SaveLinearKey(Option<SecretString>),
    /// Move a Linear issue to another workflow state, then sync Linear.
    MoveLinearIssue(super::linear::Move),
    /// Read what the new issue dialog offers, or create the issue it asks for.
    LinearCreate(linear_create::Ask),
}

pub struct Service {
    commands: SyncSender<Command>,
    mailbox: Arc<Mutex<Option<Snapshot>>>,
    notices: Arc<Mutex<VecDeque<Notice>>>,
    cancelled: Arc<AtomicBool>,
}

impl Service {
    pub fn load_agents(&self, endpoint: Option<String>, target: ConnectTarget) -> bool {
        self.commands
            .try_send(Command::LoadAgents { endpoint, target })
            .is_ok()
    }
    pub fn start(request: Request) -> std::io::Result<Self> {
        let (service, worker, receiver) = Self::prepare(request);
        std::thread::Builder::new()
            .name("herdr-orchestrator".into())
            .spawn(move || worker.run(receiver))?;
        Ok(service)
    }

    /// The service and the worker it will talk to, not yet started.
    fn prepare(request: Request) -> (Self, Worker, mpsc::Receiver<Command>) {
        let (commands, receiver) = mpsc::sync_channel(COMMANDS);
        let worker = Worker::new(request, commands.clone());
        let service = Self {
            mailbox: worker.mailbox.clone(),
            notices: worker.notices.clone(),
            cancelled: worker.cancelled.clone(),
            commands,
        };
        (service, worker, receiver)
    }

    /// A service whose commands queue and are never served, for view tests
    /// that ask for work without a worker. Its queue is leaked, so asks
    /// succeed until it is full.
    #[cfg(any(test, feature = "test-support"))]
    pub fn unserved() -> Self {
        let (commands, receiver) = mpsc::sync_channel(COMMANDS);
        std::mem::forget(receiver);
        Self {
            commands,
            mailbox: Arc::default(),
            notices: Arc::default(),
            cancelled: Arc::default(),
        }
    }

    /// Asks for `action`. A full queue means the worker is behind; the user
    /// is told, and nothing is queued silently.
    pub fn act(&self, action: Action) -> bool {
        self.commands.try_send(Command::Act(action)).is_ok()
    }

    /// Asks for pull request `number`'s status; a full queue drops it.
    pub fn load_pull_request(&self, number: u64) {
        let _ = self.commands.try_send(Command::LoadPullRequest(number));
    }

    /// Outcomes of finished actions since the last call.
    pub fn notices(&self) -> Vec<Notice> {
        self.notices
            .lock()
            .map(|mut notices| notices.drain(..).collect())
            .unwrap_or_default()
    }

    /// Asks for a sync now. A full queue already holds one, so it is dropped.
    pub fn refresh(&self, token: Option<Tokens>) {
        match self.commands.try_send(Command::Refresh(token)) {
            Ok(()) | Err(TrySendError::Full(_) | TrySendError::Disconnected(_)) => {}
        }
    }

    /// The newest snapshot since the last call, if any.
    pub fn poll(&self) -> Option<Snapshot> {
        self.mailbox.lock().ok()?.take()
    }
}

impl Drop for Service {
    fn drop(&mut self) {
        // The worker sees this at its next step or script check; it is never
        // joined here, on the UI thread.
        self.cancelled.store(true, Ordering::Relaxed);
    }
}

struct Worker {
    request: Request,
    mailbox: Arc<Mutex<Option<Snapshot>>>,
    notices: Arc<Mutex<VecDeque<Notice>>>,
    /// The worker's own queue, for action threads to report back on.
    commands: SyncSender<Command>,
    actions: Arc<AtomicUsize>,
    site: Option<Site>,
    cancelled: Arc<AtomicBool>,
    snapshot: Snapshot,
    store: Option<Store>,
    host: Option<Host>,
    /// The host could not be reached to find the repository: what was shown
    /// stays, beside the failure, and each sync looks again first.
    unreached: bool,
    /// The branches were read with the repository, so the sync that follows
    /// need not read them again.
    branches_fresh: bool,
    /// SQLite's `data_version` at the last load: it changes when another
    /// connection commits, so unchanged means nothing to reload.
    data_version: Option<i64>,
    retry: Retry,
    /// Listings shown but not written: capped, the database is newer, or
    /// Linear's, which is never written.
    uncached: HashMap<SourceKey, Vec<Item>>,
    /// Where each GitHub source's next incremental sync continues.
    resume: HashMap<SourceKey, github::Resume>,
    /// The GitLab token, once looked up, and the instance it is for; Refresh
    /// and a refusal look again, so a new `glab auth login` or a renewed
    /// connected account is picked up without reopening the view.
    gitlab_token: Option<(String, Option<gitlab::account::Found>)>,
    cooldown: Option<Duration>,
    linear_key: LinearKey,
    /// The Linear workspace's `urlKey`, once a sync has read it.
    linear_workspace: Option<String>,
    /// When Linear last began a sync.
    linear_synced: Option<Instant>,
    /// Every team's workflow states, once read; see [`linear::STATES`].
    linear_states: Option<super::linear::States>,
    /// Sends a Linear query; tests answer without a network.
    linear_fetch: fn(&SecretString, &str, Value) -> Result<Value>,
}

impl Worker {
    fn new(request: Request, commands: SyncSender<Command>) -> Self {
        Self {
            request,
            mailbox: Arc::default(),
            notices: Arc::default(),
            commands,
            actions: Arc::new(AtomicUsize::new(0)),
            site: None,
            cancelled: Arc::new(AtomicBool::new(false)),
            snapshot: Snapshot::default(),
            store: None,
            host: None,
            unreached: false,
            branches_fresh: false,
            data_version: None,
            retry: Retry::default(),
            uncached: HashMap::new(),
            resume: HashMap::new(),
            gitlab_token: None,
            cooldown: None,
            linear_key: LinearKey::Missing,
            linear_workspace: None,
            linear_synced: None,
            linear_states: None,
            linear_fetch: super::linear::post,
        }
    }

    fn run(mut self, commands: mpsc::Receiver<Command>) {
        if let Err(error) = self.open() {
            self.snapshot.error = Some(Arc::new(error));
            self.publish();
            return;
        }
        self.read_linear_key();
        // The host was just found unreachable: the next sync looks again.
        if !self.unreached {
            let token = self.request.token.clone();
            self.sync(token.as_ref(), Linear::Due);
        }
        let Timing { reload, sync, .. } = self.request.timing;
        let mut next_sync = Instant::now() + sync;
        while !self.cancelled() {
            let wait = reload.min(next_sync.saturating_duration_since(Instant::now()));
            match commands.recv_timeout(wait) {
                Ok(Command::Refresh(token)) => {
                    self.request.token = token;
                    self.gitlab_token = None;
                    self.retry.clear();
                    self.read_linear_key();
                    let token = self.request.token.clone();
                    self.sync(token.as_ref(), Linear::Now);
                    next_sync = Instant::now() + sync;
                }
                Ok(Command::Act(action)) => self.act(action),
                Ok(Command::SyncLinear) => self.sync_linear_now(),
                Ok(Command::SaveLinearKey(key)) => {
                    self.save_linear_key(key.as_ref());
                    self.sync_linear_now();
                }
                Ok(Command::MoveLinearIssue(change)) => {
                    if self.move_linear_issue(&change) {
                        self.sync_linear_now();
                    }
                }
                Ok(Command::LinearCreate(ask)) => {
                    if self.linear_create(ask) {
                        self.sync_linear_now();
                    }
                }
                Ok(Command::LoadPullRequest(number)) => self.load_pull_request(number),
                Ok(Command::LoadAgents { endpoint, target }) => {
                    self.snapshot.installed_endpoint = endpoint;
                    self.snapshot.installed = None;
                    self.snapshot.installed_error = None;
                    self.publish();
                    let result = Host::new(&target)
                        .map_err(|_| Error::UnsupportedHost)
                        .and_then(|host| {
                            ssh_slots::run(&target, &self.cancelled, || {
                                installed(Some(&host), &self.cancelled)
                            })
                        });
                    match result {
                        Ok(agents) => self.snapshot.installed = Some(Arc::new(agents)),
                        Err(error) => self.snapshot.installed_error = Some(Arc::new(error)),
                    }
                    self.publish();
                }
                Ok(Command::Acted { resync }) => {
                    if resync {
                        let token = self.request.token.clone();
                        self.sync(token.as_ref(), Linear::Due);
                    } else {
                        self.reload_if_changed();
                    }
                }
                Err(RecvTimeoutError::Timeout) if Instant::now() >= next_sync => {
                    let token = self.request.token.clone();
                    self.sync(token.as_ref(), Linear::Due);
                    next_sync = Instant::now() + sync;
                }
                Err(RecvTimeoutError::Timeout) => self.reload_if_changed(),
                Err(RecvTimeoutError::Disconnected) => return,
            }
        }
    }

    /// Starts `action` on a thread of its own, at most [`MAX_ACTIONS`] at once.
    fn act(&mut self, action: Action) {
        let Some(site) = self.site.clone() else {
            self.notice(Err(Arc::new(Error::NotARepository)));
            return;
        };
        if self.snapshot.access != Some(Access::ReadWrite) {
            let found = match self.snapshot.access {
                Some(Access::ReadOnly { found }) => found,
                _ => 0,
            };
            self.notice(Err(Arc::new(Error::NewerSchema {
                found,
                known: super::SCHEMA_VERSION,
            })));
            return;
        }
        if self.actions.fetch_add(1, Ordering::SeqCst) >= MAX_ACTIONS {
            self.actions.fetch_sub(1, Ordering::SeqCst);
            self.notice(Err(Arc::new(Error::Busy)));
            return;
        }
        let resync = matches!(action, Action::DeleteBead { .. });
        let (notices, commands, actions) = (
            self.notices.clone(),
            self.commands.clone(),
            self.actions.clone(),
        );
        let spawned = spawn_action(move |cancelled| {
            let outcome = super::actions::perform(&site, &action, cancelled).map_err(Arc::new);
            push(&notices, Notice { outcome });
            actions.fetch_sub(1, Ordering::SeqCst);
            let _ = commands.try_send(Command::Acted { resync });
        });
        if let Err(error) = spawned {
            self.actions.fetch_sub(1, Ordering::SeqCst);
            self.notice(Err(Arc::new(Error::Worker(error))));
        }
    }

    fn load_pull_request(&mut self, number: u64) {
        let remote = self
            .snapshot
            .repo
            .as_ref()
            .and_then(|info| info.remote.as_ref())
            .filter(|remote| remote.source.provider == Provider::Github);
        let (Some(remote), Some(token)) = (remote, self.request.token.clone()) else {
            return;
        };
        let Some((owner, repo)) = remote.source.repository.split_once('/') else {
            return;
        };
        let cancelled = self.cancelled.clone();
        let result = herdr_github::pull_request::by_number(
            owner,
            repo,
            number,
            token.for_owner(owner),
            || cancelled.load(Ordering::Relaxed),
            &mut self.cooldown,
        );
        self.snapshot.pull_request = Some(PullRequestLookup {
            number,
            result: Arc::new(result),
        });
        self.publish();
    }

    fn notice(&self, outcome: std::result::Result<&'static str, Arc<Error>>) {
        push(&self.notices, Notice { outcome });
    }

    fn cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Relaxed)
    }

    fn publish(&self) {
        if let Ok(mut mailbox) = self.mailbox.lock() {
            *mailbox = Some(self.snapshot.clone());
        }
    }

    /// Shows what the last run found at once, the repository it detected
    /// and its database, then finds the repository again: git may answer
    /// slowly, over SSH most of all.
    fn open(&mut self) -> Result<()> {
        let host = Host::new(&self.request.target).map_err(|_| Error::UnsupportedHost)?;
        let key = cache_key(&self.request.target, &self.request.checkout);
        let cached = self.cache(false).and_then(|cache| {
            cache
                .get::<RepoInfo>(Kind::Repository, &key)
                .inspect_err(|error| tracing::debug!(%error, "Cannot read a cached repository"))
                .ok()
                .flatten()
                .map(|entry| entry.value)
        });
        if let Some(info) = cached
            && let Err(error) = self.show(info, &host)
        {
            // `show` may have set the repository before failing: drop it, so
            // `find` shows it again and reports what failed.
            tracing::debug!(%error, "Cannot show the cached repository");
            self.forget_repository();
        }
        self.find(&host)
    }

    /// Finds the repository in the checkout, and shows and caches it unless
    /// it is what is shown. A checkout that holds another repository now, or
    /// none, replaces or drops what was shown. A host that could not be
    /// reached answered nothing about the checkout, so what was shown stays,
    /// with the failure beside it, until a later sync finds it.
    fn find(&mut self, host: &Host) -> Result<()> {
        let target = &self.request.target;
        let binaries = AgentKind::ALL.map(AgentKind::binary);
        let detected = ssh_slots::run(target, &self.cancelled, || {
            repo::detect_opening(
                target,
                host,
                &self.request.checkout,
                &binaries,
                &self.cancelled,
            )
        });
        self.found(host, detected)
    }

    /// Acts on what [`Worker::find`] detected.
    fn found(&mut self, host: &Host, detected: Result<(RepoInfo, Extras)>) -> Result<()> {
        let cache = self.cache(false);
        let key = cache_key(&self.request.target, &self.request.checkout);
        let (mut info, extras) = match detected {
            Ok(found) => found,
            Err(Error::Script { operation, source }) if source.host_unreached() => {
                self.unreached = true;
                self.snapshot.error = Some(Arc::new(Error::Script { operation, source }));
                self.publish();
                return Ok(());
            }
            Err(error) => {
                if !matches!(error, Error::Cancelled) {
                    self.unreached = false;
                    self.forget_repository();
                    if let Some(cache) = &cache
                        && let Err(error) = cache.remove(Kind::Repository, &key)
                    {
                        tracing::debug!(%error, "Cannot drop a cached repository");
                    }
                }
                return Err(error);
            }
        };
        recognize(&mut info, gitlab::cli_hosts::read);
        if std::mem::take(&mut self.unreached) {
            self.snapshot.error = None;
        }
        if self.snapshot.repo.as_ref() != Some(&info) {
            self.show(info.clone(), host)?;
            if let Some(cache) = cache.or_else(|| self.cache(true))
                && let Err(error) = cache.put(Kind::Repository, &key, &info)
            {
                tracing::debug!(%error, "Cannot cache the repository");
            }
        }
        if let Some(branches) = extras.branches {
            self.snapshot.branches = Arc::new(branches);
            self.branches_fresh = true;
        }
        let found = match extras.programs {
            Some(binaries) => Ok(agents(&binaries)),
            None => ssh_slots::run(&self.request.target, &self.cancelled, || {
                installed(self.host.as_ref(), &self.cancelled)
            }),
        };
        match found {
            Ok(agents) => self.snapshot.installed = Some(Arc::new(agents)),
            Err(error) => self.snapshot.installed_error = Some(Arc::new(error)),
        }
        self.publish();
        Ok(())
    }

    /// The app's cache database, beside the databases in `data_root` when
    /// a test sets one; only made when `create` is set, so a folder outside
    /// Git leaves nothing behind. Without it nothing is cached and nothing
    /// fails.
    fn cache(&self, create: bool) -> Option<Cache> {
        let path = match &self.request.data_root {
            Some(root) => crate::cache::path_in(root),
            None => crate::cache::path()
                .inspect_err(|error| tracing::debug!(%error, "No cache"))
                .ok()?,
        };
        if !create && !path.is_file() {
            return None;
        }
        Cache::open_at(&path)
            .inspect_err(|error| tracing::debug!(%error, "Cannot open the cache"))
            .ok()
    }

    /// Drops a repository shown from the cache that git no longer finds.
    fn forget_repository(&mut self) {
        self.store = None;
        self.site = None;
        self.data_version = None;
        self.snapshot = Snapshot::default();
    }

    /// Opens `info`'s database, carrying agent-launcher's over the first
    /// time, and publishes what it holds.
    fn show(&mut self, info: RepoInfo, host: &Host) -> Result<()> {
        let (path, legacy) = match &self.request.data_root {
            Some(root) => (
                database_path_in(root, &info.repository),
                Some(legacy_path_in(root, &info.repository)),
            ),
            None => (
                database_path(&info.repository).ok_or(Error::NoDataDirectory)?,
                legacy_path(&info.repository),
            ),
        };
        if let Some(legacy) = legacy.filter(|legacy| !path.exists() && legacy.is_file())
            && let Err(error) = Store::import(&legacy, &path)
        {
            tracing::warn!(
                category = "orchestrator",
                %error,
                "Cannot carry over agent-launcher's database"
            );
        }
        let store = Store::open(&path)?;
        self.snapshot.access = Some(store.access());
        self.snapshot.sources = statuses(&info, self.linear_source());
        self.site = Some(Site {
            host: host.clone(),
            target: self.request.target.clone(),
            main_root: info.main_root.clone(),
            database: path,
        });
        if let Some(home) = std::env::var_os("HOME") {
            self.snapshot.profiles = Arc::new(prompt::load_profiles(std::path::Path::new(&home)));
        }
        self.snapshot.repo = Some(info);
        // The checks last read, until the first sync reads them again.
        self.snapshot.pr_status = Arc::new(store.pr_statuses().unwrap_or_else(|error| {
            tracing::debug!(%error, "Cannot read stored pull request checks");
            HashMap::new()
        }));
        self.store = Some(store);
        self.host = Some(host.clone());
        self.reload()?;
        self.publish();
        Ok(())
    }

    /// Reads items and runs again, then publishes.
    fn reload(&mut self) -> Result<()> {
        let Some(store) = &self.store else {
            return Ok(());
        };
        self.data_version = Some(store.data_version()?);
        let mut items = store.items()?;
        if !self.uncached.is_empty() {
            items.retain(|item| !self.uncached.contains_key(&item.key.source));
            items.extend(self.uncached.values().flatten().cloned());
        }
        self.snapshot.items = Arc::new(items);
        self.snapshot.runs = Arc::new(store.runs()?);
        self.snapshot.sessions = Arc::new(store.sessions()?);
        Ok(())
    }

    fn reload_if_changed(&mut self) {
        let Some(store) = &self.store else {
            return;
        };
        let changed = match store.data_version() {
            Ok(version) => Some(version) != self.data_version,
            Err(_) => true,
        };
        if changed {
            if let Err(error) = self.reload() {
                self.snapshot.error = Some(Arc::new(error));
            }
            self.publish();
        }
    }

    /// Reads the shown remote as GitLab once glab knows its host, such as
    /// through an `ssh_host` added after the repository was found.
    fn recognize(&mut self, cli: impl FnOnce() -> Vec<gitlab::cli_hosts::CliHost>) {
        let Some(mut info) = self.snapshot.repo.clone() else {
            return;
        };
        if recognize(&mut info, cli) {
            self.snapshot.sources = statuses(&info, self.linear_source());
            self.snapshot.repo = Some(info);
        }
    }

    fn set_state(&mut self, key: &SourceKey, state: SyncState) {
        if let Some(status) = self.snapshot.sources.iter_mut().find(|s| s.key == *key) {
            status.state = state;
        }
    }
}

/// The app cache's key for the repository `checkout` on `target` holds.
pub fn cache_key(target: &ConnectTarget, checkout: &str) -> String {
    format!("{target:?}\n{checkout}")
}

/// Once accepted, an action completes even when its view stops observing it.
fn spawn_action(
    action: impl FnOnce(&AtomicBool) + Send + 'static,
) -> std::io::Result<std::thread::JoinHandle<()>> {
    std::thread::Builder::new()
        .name("herdr-orchestrator-action".into())
        .spawn(move || action(&AtomicBool::new(false)))
}

mod linear;
pub mod linear_create;
#[cfg(test)]
mod tests;

/// Adds `notice`, dropping the oldest past [`MAX_NOTICES`].
fn push(notices: &Mutex<VecDeque<Notice>>, notice: Notice) {
    if let Ok(mut notices) = notices.lock() {
        if notices.len() >= MAX_NOTICES {
            notices.pop_front();
        }
        notices.push_back(notice);
    }
}

/// The agents installed on `host`, in [`AgentKind::ALL`] order.
fn installed(host: Option<&Host>, cancelled: &AtomicBool) -> Result<Vec<AgentKind>> {
    let binaries = AgentKind::ALL.map(AgentKind::binary);
    let found = host
        .ok_or(Error::UnsupportedHost)?
        .installed_programs(&binaries, cancelled)
        .map_err(super::error::script("Discovering installed agents"))?;
    Ok(agents(&found))
}

/// The agents whose binaries are among `found`.
fn agents(found: &[String]) -> Vec<AgentKind> {
    AgentKind::ALL
        .into_iter()
        .filter(|kind| found.iter().any(|binary| binary == kind.binary()))
        .collect()
}
