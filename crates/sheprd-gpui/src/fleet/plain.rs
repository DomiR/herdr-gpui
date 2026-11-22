//! Plain hosts, Sheprd's side (`docs/relay-plan.md`, Fleet, Spreading a
//! host, step 3): sharing a machine with no Sheprd daemon by installing
//! its relay agent, and keeping each agent's access file current. The
//! daemon computes the file (`Ask::AgentAccess`), so the log never leaves
//! it; this app only carries the file over SSH.

use super::{Error, Fleet, Machine, Standing, agent};
use crate::{remote::supervisor::Client, teleport::Host};
use gpui::{App, BackgroundExecutor};
use sheprd_client::ConnectTarget;
use sheprd_relay_agent::access::Access;
use sheprd_remote_control::control::fleet::{
    HostKind, View,
    sync::{self, Answer, Ask},
};
use std::sync::atomic::AtomicBool;

/// This computer's relays with its listen tokens for them, for the agent
/// to listen on. Blocking: the tokens are read from the daemon's file.
fn with_tokens(relays: &[String]) -> Vec<agent::Relay> {
    let mut tokens = crate::remote::relay_token::shared(relays);
    relays
        .iter()
        .map(|url| agent::Relay {
            url: url.clone(),
            token: tokens.remove(url),
        })
        .collect()
}

/// This computer's relays, from its own host record: the agent listens on
/// them, so a member that reaches this computer reaches the host.
pub(crate) fn relays(view: &View) -> Vec<String> {
    view.hosts
        .iter()
        .find(|host| host.kind == HostKind::Member && host.added_by == view.me)
        .map(|host| {
            host.relays
                .iter()
                .filter(|relay| agent::plain_relay(relay))
                .cloned()
                .collect()
        })
        .unwrap_or_default()
}

/// The SSH target a plain host is reached by, when its sessions check the
/// host key strictly: the host keys they accept come from the known hosts
/// of that name. A target the user chose not to check
/// (`sheprd_client::HostKeys::Skip`) is refused, since nothing it reports
/// about itself was said over a session that proved who it is.
pub(crate) fn strict_target(target: &ConnectTarget) -> Result<String, Error> {
    let ConnectTarget::Ssh { target, .. } = target else {
        return Err(Error::Unsupported);
    };
    match sheprd_client::HostKeys::of(target) {
        sheprd_client::HostKeys::Verify => Ok(target.clone()),
        sheprd_client::HostKeys::Skip => Err(Error::Unchecked),
    }
}

fn known_keys(target: &str) -> Result<Vec<String>, Error> {
    sheprd_client::known_host_keys(target).map_err(Error::Trust)
}

/// Shares `machine`, which runs no Sheprd daemon, as a plain host, leaving
/// out the computer members in `deny` (a person here asked for it, after
/// being told the agent is installed): installs or updates its agent on
/// this computer's relays, checks the host key it reports against this
/// computer's known hosts, has the daemon add it, and hands it its first
/// access file.
pub(crate) fn share(
    cx: &mut App,
    id: String,
    machine: Machine,
    deny: Vec<String>,
    done: impl FnOnce(Option<String>, &mut App) + 'static,
) {
    let (Some(client), Some(view)) = (
        crate::remote::Supervisor::client(cx),
        Fleet::view(cx).cloned(),
    ) else {
        done(Some(Error::NoDaemon.to_string()), cx);
        return;
    };
    Fleet::set_acting(cx, &id, true);
    let executor = cx.background_executor().clone();
    cx.spawn(async move |cx| {
        let outcome = async {
            // Before anything runs on the machine.
            let target = strict_target(&machine.target)?;
            let relays = relays(&view);
            if relays.is_empty() {
                return Err(Error::NoRelays);
            }
            let (connect, label) = (machine.target.clone(), machine.label.clone());
            let installing = relays.clone();
            let (report, user, known) = executor
                .spawn(async move {
                    let host = Host::new(&connect).map_err(|_| Error::Unsupported)?;
                    let cancelled = AtomicBool::new(false);
                    let user = agent::user(&host, &cancelled)?;
                    let report = agent::install(&host, &with_tokens(&installing), &cancelled)?;
                    let known = known_keys(&target)?;
                    // Every key the record carries must be one the host
                    // negotiates in a strictly checked session, which also
                    // honours the user's `RevokedHostKeys`, not only one
                    // known_hosts lists and the host says it has.
                    let mut confirmed = Vec::new();
                    for key in agent::trusted_keys(&report.ssh_host_keys, &known) {
                        if sheprd_client::confirm_host_key(&target, &key).map_err(Error::Trust)? {
                            confirmed.push(key);
                        }
                    }
                    Ok::<_, Error>((report, user, confirmed))
                })
                .await?;
            let intro = agent::intro(&report, &known, &label, &user, &relays, now_ms())?;
            let host = intro.id().map_err(Error::Intro)?;
            let bytes = intro.to_bytes().map_err(Error::Intro)?;
            daemon(
                &client,
                Ask::AddHost {
                    intro: sync::text(&bytes),
                    deny,
                },
            )
            .await?;
            keep(&client, &executor, &machine, &host.to_string(), &report).await
        }
        .await;
        cx.update(|cx| {
            Fleet::set_acting(cx, &id, false);
            if outcome.is_ok() {
                Fleet::set_link(cx, &id, |link| link.standing = Standing::Agent);
            }
            Fleet::refresh(cx);
            done(outcome.err().as_ref().map(shown), cx);
        });
    })
    .detach();
}

/// What a person reads when sharing fails: why no agent build could go to
/// the host in their language, the rest as the fleet words it.
pub(crate) fn shown(error: &Error) -> String {
    use crate::i18n::errors::Localize as _;
    match error {
        Error::AgentBuild(error) => error.localize(),
        Error::Build(error) => error.localize(),
        other => other.to_string(),
    }
}

/// Where a machine with no Sheprd daemon stands: a plain host of this
/// fleet, kept current; or one with no agent of this fleet, which may be
/// shared as one; and the host its agent serves, once its key names it.
/// Runs from the sync worker.
pub(super) async fn sync(
    client: &Client,
    executor: &BackgroundExecutor,
    machine: &Machine,
    view: &View,
) -> Result<(Standing, Option<String>), Error> {
    // Nothing is asked of a machine whose key is not checked.
    let target = match strict_target(&machine.target) {
        Ok(target) => target,
        Err(Error::Unchecked) => return Ok((Standing::Unchecked, None)),
        Err(_) => return Ok((Standing::NoDaemon, None)),
    };
    let connect = machine.target.clone();
    let report = executor
        .spawn(async move {
            let host = Host::new(&connect).map_err(|_| Error::Unsupported)?;
            agent::status(&host, &AtomicBool::new(false))
        })
        .await?;
    let Some(report) = report else {
        return Ok((Standing::NoDaemon, None));
    };
    if let Some(standing) = not_ours(report.fleet.as_ref(), &view.fleet) {
        return Ok((standing, None));
    }
    let reported = report.ssh_host_keys.clone();
    let known = executor.spawn(async move { known_keys(&target) }).await?;
    let Some(host) = agent::host_id(&reported, &known) else {
        return Ok((Standing::NoDaemon, None));
    };
    let host = host.to_string();
    // A host removed everywhere is gone from the view, but its agent still
    // holds this fleet's access until it is told to forget it.
    let removed = if live(view, &host) {
        Vec::new()
    } else {
        match daemon(client, Ask::RemovedHosts).await? {
            Answer::Hosts { hosts } => hosts,
            _ => Vec::new(),
        }
    };
    let standing = match next(view, &host, &removed) {
        Next::Keep => {
            keep(client, executor, machine, &host, &report).await?;
            Standing::Agent
        }
        Next::Forget => {
            let connect = machine.target.clone();
            executor
                .spawn(async move {
                    let host = Host::new(&connect).map_err(|_| Error::Unsupported)?;
                    agent::scrub(&host, &AtomicBool::new(false))
                })
                .await?;
            Standing::Removed
        }
        Next::Leave => Standing::NoDaemon,
    };
    // The host, proved by its key, whatever came of it.
    Ok((standing, Some(host)))
}

/// What the sync does with an agent of this fleet on host `host`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Next {
    /// A plain host of the fleet: keep its access file current.
    Keep,
    /// A host the fleet removed: have its agent forget the fleet.
    Forget,
    /// Neither: left for a person.
    Leave,
}

fn live(view: &View, host: &str) -> bool {
    view.hosts
        .iter()
        .any(|entry| entry.id == host && entry.kind == HostKind::Agent)
}

/// What to do with an agent of this fleet on `host`, given the hosts the
/// fleet removed (`removed`, hex).
pub(crate) fn next(view: &View, host: &str, removed: &[String]) -> Next {
    if live(view, host) {
        Next::Keep
    } else if removed.iter().any(|gone| gone == host) {
        Next::Forget
    } else {
        Next::Leave
    }
}

/// Whether the agent holding `held` needs nothing from `file`, whose
/// digest is `digest`: it holds that file, has nothing to settle, and no
/// member's new keys wait for room on the relays, which may have freed
/// with the digest unchanged, and it did not refuse the last file it was
/// handed, which shuts the fleet out until a good one arrives.
pub(crate) fn current(
    held: Option<&sheprd_relay_agent::fleet::Report>,
    digest: &str,
    file: &Access,
) -> bool {
    held.is_some_and(|held| held.digest == digest && held.crowded.is_empty() && !held.refused)
        && file.settled.is_empty()
}

/// Hands the agent on `machine` the access file the daemon computes for
/// `host`, settling what it reported suspending, unless it holds that file
/// already and has nothing to settle.
async fn keep(
    client: &Client,
    executor: &BackgroundExecutor,
    machine: &Machine,
    host: &str,
    report: &sheprd_relay_agent::status::Report,
) -> Result<(), Error> {
    let held = report.fleet.as_ref();
    let suspended = held.map(|held| held.suspended.clone()).unwrap_or_default();
    let access = match daemon(
        client,
        Ask::AgentAccess {
            host: host.to_owned(),
            suspended,
        },
    )
    .await?
    {
        Answer::AgentAccess { access } => access,
        _ => return Err(Error::Daemon(String::new())),
    };
    let file = Access::parse(access.as_bytes())?;
    let digest = file.digest().map_err(sheprd_relay_agent::Error::from)?;
    if current(held, &digest, &file) {
        return Ok(());
    }
    let connect = machine.target.clone();
    executor
        .spawn(async move {
            let host = Host::new(&connect).map_err(|_| Error::Unsupported)?;
            agent::apply(&host, &access, &AtomicBool::new(false))
        })
        .await?;
    Ok(())
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| {
            u64::try_from(since.as_millis()).unwrap_or(u64::MAX)
        })
}

/// Where an agent holding `held` stands when it is not this fleet's to
/// keep current: one in no fleet, such as one whose fleet access was
/// removed (`fleet scrub`), is shared again only when a person asks; one
/// in another fleet is outside. `None` for an agent of `fleet`.
pub(crate) fn not_ours(
    held: Option<&sheprd_relay_agent::fleet::Report>,
    fleet: &str,
) -> Option<Standing> {
    match held {
        None => Some(Standing::NoDaemon),
        Some(held) if held.fleet.0 != fleet_bytes(fleet) => Some(Standing::Outside),
        Some(_) => None,
    }
}

/// A fleet id's bytes from its hex; zeros for one that is not.
fn fleet_bytes(fleet: &str) -> [u8; 32] {
    sync::fleet(fleet).map(|id| id.0).unwrap_or_default()
}

async fn daemon(client: &Client, ask: Ask) -> Result<Answer, Error> {
    super::flows::daemon(client, ask).await
}
