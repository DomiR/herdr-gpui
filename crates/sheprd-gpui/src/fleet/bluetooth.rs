//! The fleet around a Bluetooth pairing (`docs/relay-plan.md`, Adding a
//! nearby Sheprd from another network): what Add Device > Nearby Sheprd
//! asks this computer's daemon before the claim, and what it does once
//! the other computer is paired and saved. The `desktop-pair` child
//! checked everything the other computer answered; the daemon signs only
//! what a person here chose on that page.

use super::{Error, Fleet, flows, remote};
use crate::remote::Supervisor;
use gpui::App;
use sheprd_client::ConnectTarget;
use sheprd_remote_control::control::fleet::{
    HostKind, Kind, Method, View,
    bluetooth::{ApprovedFleet, Hint, Joining},
    sync::{Answer, Apply, Ask},
};
use std::{sync::atomic::AtomicBool, time::Duration};

/// The session saved devices use.
const SESSION: &str = "default";

/// `view`'s fleet as a claim names it.
pub(crate) fn hint(view: &View) -> Hint {
    let computers = view
        .members
        .iter()
        .filter(|member| member.kind == Kind::Computer)
        .count();
    let hosts = view
        .hosts
        .iter()
        .filter(|host| host.kind != HostKind::Member)
        .count();
    Hint {
        fleet: view.fleet.clone(),
        computers: u16::try_from(computers).unwrap_or(u16::MAX),
        hosts: u16::try_from(hosts).unwrap_or(u16::MAX),
    }
}

/// This computer's bundle for `fleet`, signed by its daemon and bound to
/// the pairing whose token hashes to `pairing`: the person here chose to
/// join the other computer's fleet.
pub(crate) fn joining(
    cx: &mut App,
    fleet: String,
    pairing: String,
    done: impl FnOnce(Result<Joining, Error>, &mut App) + 'static,
) {
    let Some(client) = Supervisor::client(cx) else {
        done(Err(Error::NoDaemon), cx);
        return;
    };
    cx.spawn(async move |cx| {
        let answer = flows::daemon(&client, Ask::Joining { fleet, pairing }).await;
        let joining = match answer {
            Ok(Answer::Joining(joining)) => Ok(joining),
            Ok(_) => Err(Error::Unexpected),
            Err(error) => Err(error),
        };
        cx.update(|cx| done(joining, cx));
    })
    .detach();
}

/// Founds a fleet with this computer as its only member, as the person
/// here chose, and answers it as a claim's invitation names it.
pub(crate) fn found(cx: &mut App, done: impl FnOnce(Result<Hint, Error>, &mut App) + 'static) {
    let Some(client) = Supervisor::client(cx) else {
        done(Err(Error::NoDaemon), cx);
        return;
    };
    cx.spawn(async move |cx| {
        let founded = async {
            flows::daemon(&client, Ask::Found).await?;
            match flows::daemon(&client, Ask::Status).await? {
                Answer::Status { view: Some(view) } => Ok(hint(&view)),
                _ => Err(Error::Unexpected),
            }
        }
        .await;
        cx.update(|cx| {
            Fleet::refresh(cx);
            done(founded, cx);
        });
    })
    .detach();
}

/// Finishes what the pairing left for the fleet, over the SSH it opened to
/// `target`: admits the other computer's bundle and hands it the log, or
/// takes the log from the other computer that admitted this one. A
/// failure shows in Settings > Fleet; the two stay paired either way.
pub(crate) fn paired(cx: &mut App, target: String, fleet: ApprovedFleet) {
    let Some(client) = Supervisor::client(cx) else {
        Fleet::pairing_done(cx, Some(Error::NoDaemon.to_string()));
        return;
    };
    let ours = Fleet::member_of(cx);
    let target = ConnectTarget::Ssh {
        target,
        session: SESSION.to_owned(),
    };
    let executor = cx.background_executor().clone();
    cx.spawn(async move |cx| {
        // The other computer may still be starting what its Allow set off,
        // or the relays may blink: tried again a few times, as each step is
        // safe to repeat (an admitted bundle is admitted once, and a log is
        // handed over from the heads each side holds).
        let mut outcome = Err(Error::Unexpected);
        for wait in RETRIES {
            executor.timer(wait).await;
            outcome = finish(&client, &executor, &target, &fleet, ours.as_deref()).await;
            if outcome.is_ok() {
                break;
            }
        }
        cx.update(|cx| {
            Fleet::pairing_done(cx, outcome.err().map(|error| error.to_string()));
            Fleet::refresh(cx);
        });
    })
    .detach();
}

/// How long `paired` waits before each try.
const RETRIES: [Duration; 5] = [
    Duration::ZERO,
    Duration::from_secs(10),
    Duration::from_secs(30),
    Duration::from_secs(60),
    Duration::from_secs(300),
];

/// One try at what the pairing left for the fleet.
async fn finish(
    client: &crate::remote::supervisor::Client,
    executor: &gpui::BackgroundExecutor,
    target: &ConnectTarget,
    fleet: &ApprovedFleet,
    ours: Option<&str>,
) -> Result<(), Error> {
    match fleet.clone() {
        ApprovedFleet::Joining(joining) => {
            let ours = ours.ok_or(Error::NoDaemon)?;
            flows::daemon(
                client,
                Ask::Admit {
                    bundle: joining.bundle,
                    signature: joining.signature,
                    method: Method::Bluetooth,
                },
            )
            .await?;
            flows::hand_over(client, executor, target, ours).await
        }
        ApprovedFleet::Joined { fleet } => {
            take_over(client, executor, target, &fleet).await?;
            match flows::daemon(client, Ask::Status).await? {
                Answer::Status { view: Some(view) } if view.fleet == fleet => Ok(()),
                _ => Err(Error::NotAdmitted),
            }
        }
    }
}

/// Takes `fleet`'s log from `target`, which admitted this computer, one
/// answer at a time: this computer's daemon keeps the history as far as it
/// arrived and answers its heads, and takes it as its log once it holds
/// the admission, which only a fleet this computer signed itself into can.
async fn take_over(
    client: &crate::remote::supervisor::Client,
    executor: &gpui::BackgroundExecutor,
    target: &ConnectTarget,
    fleet: &str,
) -> Result<(), Error> {
    let mut ours = Vec::new();
    for _ in 0..flows::MAX_PARTS {
        let ask = Apply {
            fleet: Some(fleet.to_owned()),
            heads: ours,
            ..Apply::default()
        };
        let remote = target.clone();
        let part = executor
            .spawn(async move { remote::apply(&remote, &ask, &AtomicBool::new(false)) })
            .await?;
        if part.ops.is_empty() {
            return Ok(());
        }
        let more = part.more;
        ours = flows::applied(
            client,
            Apply {
                fleet: Some(fleet.to_owned()),
                heads: part.heads,
                ops: part.ops,
                ..Apply::default()
            },
        )
        .await?
        .heads;
        if !more {
            return Ok(());
        }
    }
    Err(Error::TooLarge)
}
