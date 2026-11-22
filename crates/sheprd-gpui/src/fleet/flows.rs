//! What Sheprd does with another machine: syncs the log with it, invites
//! it to join, or shares it as a host. Each asks this computer's daemon
//! for its part and runs the machine's part over SSH on the background
//! executor.

use super::{Error, Fleet, Machine, Standing, remote};
use crate::remote::Supervisor;
use gpui::{App, BackgroundExecutor};
use sheprd_client::ConnectTarget;
use sheprd_remote_control::control::{
    Request, Response,
    fleet::{
        HostKind, Method, saved,
        sync::{self, Answer, Applied, Apply, Ask},
    },
};
use std::{sync::atomic::AtomicBool, time::Instant};

/// Asks this computer's daemon `ask` and waits for its fleet answer.
pub(super) async fn daemon(
    client: &crate::remote::supervisor::Client,
    ask: Ask,
) -> Result<Answer, Error> {
    match client.request(Request::Fleet(Box::new(ask))).await {
        Ok(Response::Fleet(answer)) => Ok(*answer),
        Ok(Response::Error { message, .. }) => Err(Error::Daemon(message)),
        Ok(_) => Err(Error::Daemon(String::new())),
        Err(error) => Err(Error::Request(error)),
    }
}

pub(super) async fn applied(
    client: &crate::remote::supervisor::Client,
    apply: Apply,
) -> Result<Applied, Error> {
    match daemon(client, Ask::Apply(apply)).await? {
        Answer::Applied(applied) => Ok(applied),
        _ => Err(Error::Daemon(String::new())),
    }
}

/// The most parts a log is carried in: the largest log a daemon keeps,
/// one answer at a time, and one more.
pub(super) const MAX_PARTS: usize = saved::MAX_SAVED.div_ceil(sync::MAX_ANSWER_BYTES) + 1;

/// Asks `target` for what this computer lacks, from `heads`, naming no
/// fleet and with a fresh nonce: its answer, when it proves it is one of
/// `hosts` (the fleet's computers and Sheprd hosts) in `fleet`, with the
/// host it proved to be; `None` for a machine outside it, which learns
/// nothing.
async fn ask_proven(
    executor: &BackgroundExecutor,
    target: &ConnectTarget,
    heads: Vec<String>,
    fleet: &str,
    hosts: &[String],
) -> Result<Option<(Applied, String)>, Error> {
    let nonce = uuid::Uuid::new_v4().simple().to_string();
    let ask = Apply {
        fleet: None,
        heads,
        nonce: Some(nonce.clone()),
        ..Apply::default()
    };
    let target = target.clone();
    match executor
        .spawn(async move { remote::apply(&target, &ask, &AtomicBool::new(false)) })
        .await
    {
        Ok(theirs) if theirs.fleet == fleet => {
            Ok(sync::proven_host(&theirs, &nonce, hosts).map(|host| (theirs, host)))
        }
        Ok(_) => Ok(None),
        // A machine with no Sheprd daemon: the caller decides.
        Err(error) => Err(error),
    }
}

/// Hands this computer's whole log to `target`, which is in no fleet yet,
/// one answer at a time: the machine keeps the history as far as it
/// arrived, granting nothing, and answers its heads, from which this
/// computer's daemon says what comes next. It joins once the history
/// admits it, or shares it as a host, which the last part holds.
pub(super) async fn hand_over(
    client: &crate::remote::supervisor::Client,
    executor: &BackgroundExecutor,
    target: &ConnectTarget,
    fleet: &str,
) -> Result<(), Error> {
    let mut theirs = Vec::new();
    for _ in 0..MAX_PARTS {
        let part = applied(
            client,
            Apply {
                fleet: Some(fleet.to_owned()),
                heads: theirs,
                ..Apply::default()
            },
        )
        .await?;
        if part.ops.is_empty() {
            return Ok(());
        }
        let more = part.more;
        let send = Apply {
            fleet: Some(fleet.to_owned()),
            heads: part.heads,
            ops: part.ops,
            ..Apply::default()
        };
        let target = target.clone();
        theirs = executor
            .spawn(async move { remote::apply(&target, &send, &AtomicBool::new(false)) })
            .await?
            .heads;
        if !more {
            return Ok(());
        }
    }
    Err(Error::TooLarge)
}

/// Hands `host`, which the fleet removed and whose heads are `heads`, its
/// removal and that removal's ancestors it lacks, and nothing else: no
/// operation made after or beside it. Done once the host holds them
/// ([`Standing::Removed`]): it is not synced again.
async fn hand_removal(
    client: &crate::remote::supervisor::Client,
    executor: &BackgroundExecutor,
    target: &ConnectTarget,
    fleet: &str,
    host: &str,
    mut heads: Vec<String>,
) -> Result<Standing, Error> {
    for _ in 0..MAX_PARTS {
        let ask = Ask::RemovalFor {
            host: host.to_owned(),
            heads,
        };
        let Answer::Applied(part) = daemon(client, ask).await? else {
            return Err(Error::Daemon(String::new()));
        };
        if part.ops.is_empty() {
            return Ok(Standing::Removed);
        }
        let send = Apply {
            fleet: Some(fleet.to_owned()),
            heads: part.heads,
            ops: part.ops,
            ..Apply::default()
        };
        let target = target.clone();
        heads = executor
            .spawn(async move { remote::apply(&target, &send, &AtomicBool::new(false)) })
            .await?
            .heads;
    }
    Err(Error::TooLarge)
}

/// Syncs `machine` with this computer's fleet `fleet`:
///
/// 1. asks the machine for what this computer lacks, naming no fleet,
///    sending only this computer's heads and a nonce it must sign with
///    its certificate authority: a machine that is not one of the fleet's
///    hosts learns nothing and gets nothing;
/// 2. hands what it answered to this computer's daemon, which says what
///    the machine lacks, asking again while the machine has more;
/// 3. sends the machine that, one answer at a time.
///
/// A machine with no Sheprd daemon may be a plain host of this fleet: its
/// relay agent is kept current instead (`plain::sync`).
pub(super) fn sync(cx: &mut App, id: String, machine: Machine, fleet: String) {
    let Some(client) = Supervisor::client(cx) else {
        return;
    };
    // The host this machine last proved to be, if any: one the fleet
    // removed that now answers as an outsider has forgotten the fleet.
    let mut was = None;
    Fleet::set_link(cx, &id, |link| {
        link.running = true;
        link.last = Some(Instant::now());
        was = link.host.clone();
    });
    let executor = cx.background_executor().clone();
    cx.spawn(async move |cx| {
        let mut proven_as = None;
        let outcome = async {
            let view = match daemon(&client, Ask::Status).await? {
                Answer::Status { view: Some(view) } if view.fleet == fleet => view,
                _ => return Ok((Standing::Outside, Vec::new(), Vec::new())),
            };
            // Hosts that sign with a certificate authority: computers in
            // the fleet and Sheprd hosts. Only one of them may prove it is
            // the machine answering.
            let mut hosts: Vec<String> = view
                .hosts
                .iter()
                .filter(|host| host.kind != HostKind::Agent)
                .map(|host| host.id.clone())
                .collect();
            // A host the fleet removed still proves itself, so it is
            // handed its removal, and nothing else.
            let removed = match daemon(&client, Ask::RemovedHosts).await {
                Ok(Answer::Hosts { hosts }) => hosts,
                _ => Vec::new(),
            };
            hosts.extend(removed.iter().cloned());
            let target = &machine.target;
            let answered =
                match ask_proven(&executor, target, view.heads.clone(), &fleet, &hosts).await {
                    // No Sheprd daemon: perhaps a plain host of this fleet.
                    Err(Error::NoProgram) => {
                        let (standing, host) =
                            super::plain::sync(&client, &executor, &machine, &view).await?;
                        // The host its agent serves: what a removal and a
                        // later share again are recognized by.
                        proven_as = host;
                        return Ok((standing, Vec::new(), view.heads.clone()));
                    }
                    answered => answered?,
                };
            let Some((mut theirs, proven)) = answered else {
                let standing = if was.as_ref().is_some_and(|host| removed.contains(host)) {
                    Standing::Removed
                } else {
                    Standing::Outside
                };
                return Ok((standing, Vec::new(), Vec::new()));
            };
            proven_as = Some(proven.clone());
            if removed.contains(&proven) {
                let standing =
                    hand_removal(&client, &executor, target, &fleet, &proven, theirs.heads).await?;
                return Ok((standing, Vec::new(), Vec::new()));
            }
            let mut ours;
            let mut parts = 1;
            loop {
                // Named, so the daemon exports nothing should the host be
                // removed meanwhile.
                ours = applied(
                    &client,
                    Apply {
                        fleet: Some(fleet.clone()),
                        heads: theirs.heads.clone(),
                        ops: std::mem::take(&mut theirs.ops),
                        peer: Some(proven.clone()),
                        ..Apply::default()
                    },
                )
                .await?;
                if !theirs.more || parts >= MAX_PARTS {
                    break;
                }
                parts += 1;
                let (next, again) =
                    match ask_proven(&executor, target, ours.heads.clone(), &fleet, &hosts).await {
                        Ok(Some(answer)) => answer,
                        Ok(None) | Err(Error::NoProgram) => {
                            return Ok((Standing::Outside, Vec::new(), Vec::new()));
                        }
                        Err(error) => return Err(error),
                    };
                if again != proven {
                    return Ok((Standing::Outside, Vec::new(), Vec::new()));
                }
                theirs = next;
            }
            let mut heads = theirs.heads;
            for _ in 0..MAX_PARTS {
                if ours.ops.is_empty() {
                    break;
                }
                let more = ours.more;
                let send = Apply {
                    fleet: Some(fleet.clone()),
                    heads: ours.heads.clone(),
                    ops: ours.ops,
                    ..Apply::default()
                };
                let target = target.clone();
                heads = executor
                    .spawn(async move { remote::apply(&target, &send, &AtomicBool::new(false)) })
                    .await?
                    .heads;
                if !more {
                    break;
                }
                ours = applied(
                    &client,
                    Apply {
                        fleet: Some(fleet.clone()),
                        heads: heads.clone(),
                        peer: Some(proven.clone()),
                        ..Apply::default()
                    },
                )
                .await?;
            }
            // A shared Sheprd host is no member and cannot publish its own
            // record when its relays move: this computer carries it.
            let shared = view
                .hosts
                .iter()
                .any(|host| host.id == proven && host.kind == HostKind::Sheprd);
            if shared {
                let target = target.clone();
                let seen = executor
                    .spawn(async move { remote::intro(&target, &AtomicBool::new(false)) })
                    .await;
                let carried = match seen {
                    Ok(intro) => daemon(&client, Ask::Seen { intro }).await.map(drop),
                    Err(error) => Err(error),
                };
                // The log itself synced: a record not carried now is
                // tried again on the next sync.
                if let Err(error) = carried {
                    tracing::debug!(%error, "Could not carry a shared host's record");
                }
            }
            Ok::<_, Error>((Standing::Member, heads, ours.heads))
        }
        .await;
        cx.update(|cx| {
            match outcome {
                Ok((standing, heads, synced)) => Fleet::set_link(cx, &id, |link| {
                    link.settle(standing, heads, synced, proven_as);
                }),
                Err(error) => {
                    tracing::debug!(%error, "Could not sync the fleet with a machine");
                    Fleet::set_link(cx, &id, |link| {
                        link.running = false;
                        link.standing = Standing::Failed;
                    });
                }
            }
            Fleet::refresh(cx);
        });
    })
    .detach();
}

/// Invites the computer `machine` to join: it signs its own bundle for the
/// fleet, this computer's daemon admits it (a person here asked for it),
/// and the whole log is handed over, which the machine takes only because
/// it admits it.
pub(crate) fn invite(
    cx: &mut App,
    id: String,
    machine: Machine,
    done: impl FnOnce(Option<String>, &mut App) + 'static,
) {
    let (Some(client), Some(fleet)) = (
        Supervisor::client(cx),
        Fleet::view(cx).map(|view| view.fleet.clone()),
    ) else {
        done(Some(Error::NoDaemon.to_string()), cx);
        return;
    };
    Fleet::set_acting(cx, &id, true);
    let executor = cx.background_executor().clone();
    cx.spawn(async move |cx| {
        let outcome = async {
            let target = machine.target.clone();
            let named = fleet.clone();
            let (bundle, signature) = executor
                .spawn(async move { remote::bundle(&target, &named, &AtomicBool::new(false)) })
                .await?;
            daemon(
                &client,
                Ask::Admit {
                    bundle,
                    signature,
                    method: Method::Ssh,
                },
            )
            .await?;
            hand_over(&client, &executor, &machine.target, &fleet).await?;
            Ok::<_, Error>(())
        }
        .await;
        cx.update(|cx| {
            Fleet::set_acting(cx, &id, false);
            // It is in the fleet now, whatever it said before: sync it.
            if outcome.is_ok() {
                Fleet::set_link(cx, &id, super::Link::restart);
            }
            Fleet::refresh(cx);
            done(outcome.err().map(|error| error.to_string()), cx);
        });
    })
    .detach();
}

/// Shares the Sheprd host `machine` with the fleet, leaving out the
/// computer members in `deny`: it signs its own record with its
/// certificate authority, this computer's daemon adds it (a person here
/// asked for it), and the whole log is handed over, which the host takes
/// only because it shares it.
pub(crate) fn share(
    cx: &mut App,
    id: String,
    machine: Machine,
    deny: Vec<String>,
    done: impl FnOnce(Option<String>, &mut App) + 'static,
) {
    let (Some(client), Some(fleet)) = (
        Supervisor::client(cx),
        Fleet::view(cx).map(|view| view.fleet.clone()),
    ) else {
        done(Some(Error::NoDaemon.to_string()), cx);
        return;
    };
    Fleet::set_acting(cx, &id, true);
    let executor = cx.background_executor().clone();
    cx.spawn(async move |cx| {
        let outcome = async {
            let target = machine.target.clone();
            let intro = executor
                .spawn(async move { remote::intro(&target, &AtomicBool::new(false)) })
                .await?;
            daemon(&client, Ask::AddHost { intro, deny }).await?;
            // The host takes the log because it shares it, then lets in
            // the computers allowed there; syncing keeps it current.
            hand_over(&client, &executor, &machine.target, &fleet).await?;
            Ok::<_, Error>(())
        }
        .await;
        cx.update(|cx| {
            Fleet::set_acting(cx, &id, false);
            // It is in the fleet now, whatever it said before: sync it.
            if outcome.is_ok() {
                Fleet::set_link(cx, &id, super::Link::restart);
            }
            Fleet::refresh(cx);
            done(outcome.err().map(|error| error.to_string()), cx);
        });
    })
    .detach();
}

/// Removes every way a fleet reaches `machine`'s account, over SSH: the
/// last resort for a host no member can reach or a fleet that is gone (a
/// person here asked for it). Signs nothing: it only takes access away.
pub(crate) fn scrub(
    cx: &mut App,
    id: String,
    machine: Machine,
    done: impl FnOnce(Option<String>, &mut App) + 'static,
) {
    Fleet::set_acting(cx, &id, true);
    let target = machine.target;
    let task = cx
        .background_executor()
        .spawn(async move { remote::scrub(&target, &AtomicBool::new(false)) });
    cx.spawn(async move |cx| {
        let outcome = task.await;
        cx.update(|cx| {
            Fleet::set_acting(cx, &id, false);
            Fleet::set_link(cx, &id, |link| *link = super::Link::default());
            Fleet::refresh(cx);
            done(outcome.err().map(|error| error.to_string()), cx);
        });
    })
    .detach();
}
