//! One owner for every document link, independent of its open editors.

mod lifecycle;
mod observe;
mod persistence;
mod send;

use super::{
    Delivery, Error, Launch, Launched, LinkStatus, Mode, Pending, Push, Range, Record, Result,
};
use crate::{browser::Scope, config::DocAgentConfig, doc_view::DocBuffer, teleport::Host};
use gpui::{App, AppContext, Context, Entity, Global, Subscription, Task};
use herdr_client::{
    ConnectTarget,
    protocol::{AgentStatus, ClientShellSnapshot},
};
use herdr_doc::Version;
use persistence::Persistence;
use std::{
    path::{Path, PathBuf},
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

struct Handle(Entity<Links>);
impl Global for Handle {}

pub(crate) fn links(cx: &App) -> Option<Entity<Links>> {
    cx.try_global::<Handle>().map(|handle| handle.0.clone())
}

pub(super) struct Enable {
    pub(super) buffer: Entity<DocBuffer>,
    pub(super) reveal: Option<super::window::Reveal>,
    pub(super) scope: Scope,
    pub(super) snapshot: Arc<ClientShellSnapshot>,
    pub(super) target: ConnectTarget,
    pub(super) workspace_id: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ControlKind {
    // keep-sorted start
    Resync,
    Start,
    // keep-sorted end
}

struct Control {
    kind: ControlKind,
    retry_at: Instant,
    version: Version,
}

struct Flight {
    _task: Task<()>,
    cancelled: Arc<AtomicBool>,
}

impl Drop for Flight {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Release);
    }
}

struct Timer {
    _task: Task<()>,
    deadline: Instant,
}

struct Link {
    acked: Option<Version>,
    buffer: Option<Entity<DocBuffer>>,
    control: Option<Control>,
    error: Option<Arc<Error>>,
    failures: u32,
    generation: u64,
    job: Option<Flight>,
    last_status: Option<AgentStatus>,
    mode: Mode,
    not_ready: bool,
    off: bool,
    off_range: Option<Range>,
    path: PathBuf,
    push: Option<Push>,
    record: Option<Record>,
    retry_at: Option<Instant>,
    reveal: Option<super::window::Reveal>,
    scope: Scope,
    seen: bool,
    snapshot: Option<(Instant, Arc<ClientShellSnapshot>)>,
    subscriptions: Vec<Subscription>,
    target: ConnectTarget,
    timer: Option<Timer>,
    used: u64,
    version: Option<Version>,
    workspace_id: String,
}

struct LaunchResult {
    result: Result<Launched>,
    started: Option<Launched>,
}

type LaunchJob = dyn Fn(&Host, &Launch<'_>, &Path, &AtomicBool) -> LaunchResult + Send + Sync;
type PromptJob = dyn Fn(&Host, &str, &str, &AtomicBool) -> Result<Delivery> + Send + Sync;

struct Jobs {
    launch: Arc<LaunchJob>,
    prompt: Arc<PromptJob>,
}

impl Default for Jobs {
    fn default() -> Self {
        Self {
            launch: Arc::new(|host, launch, path, cancelled| {
                let mut started = None;
                let result = super::launch(
                    host,
                    launch,
                    || super::agent_name(path, uuid::Uuid::new_v4().as_u128() as u32),
                    |step| {
                        if let super::Step::AgentStarted(agent) = step {
                            started = Some(agent);
                        }
                    },
                    cancelled,
                );
                LaunchResult { result, started }
            }),
            prompt: Arc::new(super::prompt),
        }
    }
}

pub(crate) struct Links {
    _config: Subscription,
    config: DocAgentConfig,
    entries: Vec<Link>,
    jobs: Jobs,
    now: Rc<dyn Fn() -> Instant>,
    persistence: Persistence,
    quitting: bool,
    sequence: u64,
}

impl Links {
    pub(super) fn new(path: Option<PathBuf>, cx: &mut Context<Self>) -> Self {
        let config = cx
            .try_global::<crate::app::InitialAppearance>()
            .map(|appearance| appearance.config.doc_agent.clone())
            .unwrap_or_default();
        let observed = cx.observe_global::<crate::app::InitialAppearance>(|this, cx| {
            let config = cx
                .global::<crate::app::InitialAppearance>()
                .config
                .doc_agent
                .clone();
            this.set_config(config, cx);
        });
        // Deadlines and timer wakes must share a clock, including intermediate test ticks.
        let clock = cx.background_executor().clone();
        Self {
            _config: observed,
            config,
            entries: Vec::new(),
            jobs: Jobs::default(),
            now: Rc::new(move || clock.now()),
            persistence: Persistence::new(path),
            quitting: false,
            sequence: 0,
        }
    }

    pub(super) fn install(path: Option<PathBuf>, records: Vec<Record>, cx: &mut App) {
        if cx.has_global::<Handle>() {
            return;
        }
        let entity = cx.new(|cx| Self::new(path, cx));
        cx.set_global(Handle(entity.clone()));
        entity.update(cx, |this, cx| this.restore(records, cx));
        cx.on_app_quit(move |cx| {
            let task = entity.update(cx, |this, cx| this.shutdown(cx));
            cx.background_executor().spawn(async move {
                if let Some(task) = task {
                    task.await;
                }
            })
        })
        .detach();
    }

    pub(crate) fn status(&self, path: &Path) -> Option<LinkStatus> {
        let link = self.entries.iter().find(|link| link.path == path)?;
        let acked = link.push.as_ref().map(Push::acked).or(link.acked);
        let pending = link.push.as_ref().and_then(|push| {
            (push.deadline().is_some() || push.in_flight() || link.control.is_some()).then_some(
                Pending {
                    from: push.acked(),
                    in_flight: link.job.is_some(),
                    to: link.version.unwrap_or(push.acked()),
                },
            )
        });
        Some(LinkStatus {
            acked,
            agent_name: link.record.as_ref().map(|record| record.agent_name.clone()),
            mode: link.mode,
            pane_id: link.record.as_ref().map(|record| record.pane_id.clone()),
            pending,
        })
    }

    pub(super) fn error(&self, path: &Path) -> Option<&Error> {
        self.entries
            .iter()
            .find(|link| link.path == path)?
            .error
            .as_deref()
    }

    pub(super) fn not_ready(&self, path: &Path) -> bool {
        self.entries
            .iter()
            .any(|link| link.path == path && link.not_ready)
    }

    pub(super) fn reveal_requests(
        &self,
        window: gpui::WindowId,
    ) -> Vec<(PathBuf, super::window::Reveal, Option<Record>)> {
        self.entries
            .iter()
            .filter_map(|link| {
                let reveal = link
                    .reveal
                    .as_ref()
                    .filter(|reveal| reveal.window == window)?;
                let record = matches!(link.mode, Mode::On | Mode::Paused)
                    .then(|| link.record.clone())
                    .flatten();
                Some((link.path.clone(), reveal.clone(), record))
            })
            .collect()
    }

    pub(super) fn forget_reveal(&mut self, path: &Path) {
        if let Some(index) = self.index(path) {
            self.entries[index].reveal = None;
        }
    }

    fn set_config(&mut self, config: DocAgentConfig, cx: &mut Context<Self>) {
        if self.config == config {
            return;
        }
        self.config = config;
        for link in &mut self.entries {
            if let Some(push) = &mut link.push {
                push.set_debounce(self.config.debounce());
            }
        }
        self.pump(cx);
        cx.notify();
    }

    fn index(&self, path: &Path) -> Option<usize> {
        self.entries.iter().position(|link| link.path == path)
    }
}

#[cfg(test)]
#[path = "tests/runtime.rs"]
mod tests;
