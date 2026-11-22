//! How the daemon's iPhone pushes leave it (`Request::PushStatus`), for
//! Settings' push relay row: asked with each poll while a lease is held.
//! Each linked host's own daemon is asked too (`remote::host_push`), over
//! SSH, so at most once a minute and never on a poll's path.
use super::Supervisor;
use crate::remote::host_push;
use gpui::{App, Task};
use sheprd_control_protocol::protocol::remote::{HostPush, MAX_PUSH_HOSTS};
use sheprd_remote_control::control::{self, Response, push::PushStatus};
use std::{
    sync::atomic::AtomicBool,
    time::{Duration, Instant},
};

/// How long linked hosts' answers stand before Settings asks again.
pub(super) const HOSTS_FRESH: Duration = Duration::from_secs(60);

/// Linked hosts' push status, as last asked for Settings.
#[derive(Debug, Default)]
pub(super) struct HostPushes {
    pub(super) asked: Option<Instant>,
    pub(super) asking: bool,
    hosts: Vec<HostPush>,
}

impl HostPushes {
    /// Whether to ask the hosts again at `now`: no ask runs, and the last
    /// answers are older than [`HOSTS_FRESH`], or there are none yet.
    pub(super) fn due(&self, now: Instant) -> bool {
        !self.asking
            && self
                .asked
                .is_none_or(|at| now.saturating_duration_since(at) >= HOSTS_FRESH)
    }
}

impl Supervisor {
    /// Takes a poll's answer. Returns whether what Settings shows changed.
    /// A daemon too old to answer leaves it unknown.
    pub(super) fn push_polled(&mut self, answer: Result<Response, control::Error>) -> bool {
        let status = match answer {
            Ok(Response::PushStatus(status)) => Some(status),
            _ => None,
        };
        let changed = status != self.push_status;
        self.push_status = status;
        changed
    }

    pub(crate) fn push_status(cx: &App) -> Option<&PushStatus> {
        cx.try_global::<Self>()?.push_status.as_ref()
    }

    /// Linked hosts' push status, as last asked while Settings watched.
    pub(crate) fn host_pushes(cx: &App) -> &[HostPush] {
        cx.try_global::<Self>()
            .map_or(&[], |supervisor| supervisor.host_push.hosts.as_slice())
    }

    /// Whether there is any linked host to ask.
    pub(crate) fn has_hosts(cx: &App) -> bool {
        cx.try_global::<Self>()
            .is_some_and(|supervisor| !supervisor.links.known().is_empty())
    }

    /// Asks every linked host, at most [`MAX_PUSH_HOSTS`], all at once on
    /// the background executor, in the window's order.
    pub(crate) fn ask_hosts(cx: &App) -> Task<Vec<HostPush>> {
        let known: Vec<_> = cx
            .try_global::<Self>()
            .map(|supervisor| {
                supervisor
                    .links
                    .known()
                    .iter()
                    .take(MAX_PUSH_HOSTS)
                    .cloned()
                    .collect()
            })
            .unwrap_or_default();
        let executor = cx.background_executor().clone();
        cx.background_executor().spawn(async move {
            let asks: Vec<_> = known
                .into_iter()
                .map(|known| {
                    executor.spawn(async move {
                        let asked = host_push::ask(&known.target, &AtomicBool::new(false));
                        host_push::answer(known.host.name, asked)
                    })
                })
                .collect();
            let mut hosts = Vec::with_capacity(asks.len());
            for ask in asks {
                hosts.push(ask.await);
            }
            hosts
        })
    }

    /// Asks the linked hosts again for Settings when their answers are
    /// due ([`HostPushes::due`]). Called on every tick while Settings
    /// watches, whether or not this computer's own daemon runs.
    pub(super) fn refresh_hosts(cx: &mut App) {
        let supervisor = cx.global_mut::<Self>();
        let pushes = &mut supervisor.host_push;
        if !pushes.due(Instant::now()) {
            return;
        }
        if supervisor.links.known().is_empty() {
            pushes.hosts.clear();
            return;
        }
        pushes.asking = true;
        let task = Self::ask_hosts(cx);
        cx.spawn(async move |cx| {
            let hosts = task.await;
            cx.update(|cx| {
                let pushes = &mut cx.global_mut::<Self>().host_push;
                pushes.asking = false;
                pushes.asked = Some(Instant::now());
                if pushes.hosts != hosts {
                    pushes.hosts = hosts;
                    cx.refresh_windows();
                }
            });
        })
        .detach();
    }

    /// Takes a push status as a poll would have.
    #[cfg(test)]
    pub(crate) fn set_push_status(cx: &mut App, status: Option<PushStatus>) {
        cx.global_mut::<Self>().push_status = status;
    }

    /// Takes linked hosts' push status as an ask would have.
    #[cfg(test)]
    pub(crate) fn set_host_pushes(cx: &mut App, hosts: Vec<HostPush>) {
        cx.global_mut::<Self>().host_push.hosts = hosts;
    }
}
