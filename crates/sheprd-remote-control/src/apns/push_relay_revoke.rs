//! Revoking an unpaired device's push relay handles at the push relay
//! (`docs/apns-relay-plan.md`, 5. Revoke), so the copies this computer
//! held stop working even if they were read off its disk. This is the push
//! relay (`sheprd-push-relay`), not the host relays computers are reached
//! through.
//!
//! `Auth` reads a device's handles from its record just before the record
//! goes, whichever path removes it: Sheprd's `Revoke` (which
//! `sheprd-remote-control revoke` also sends to the running daemon), a host
//! replacing a device it enrolled, the fleet taking devices away, and idle
//! expiry. It queues them with [`queue`], which never blocks and never
//! fails the unpair; [`run`] revokes them on the Tokio side.
//!
//! Best effort, and bounded: at most [`QUEUE`] unpaired devices wait, a
//! full queue drops the newest, at most [`CONCURRENCY`] calls run at once,
//! and each call is `RelayClient::revoke`'s own few tries within seconds,
//! with no retry after that; while a push relay is out, its revocations
//! are dropped at once. A handle whose revocation is lost still runs out
//! with its credential. The log names no handle and no device.
//!
//! Only a daemon whose pushes go through the push relay revokes: one with
//! `push_relay` off makes no connection to the push relay, and its devices' handles are simply never used. A device's
//! handles replaced or dropped by its next registration are not revoked
//! either: the phone makes them, may hand them out again, and the push
//! relay allows each device only a few revocations a minute.

use super::Handle;
use sheprd_apns::RelayOutcome;
use tokio::{
    sync::mpsc::{self, error::TrySendError},
    task::{JoinError, JoinSet},
};

/// How many unpaired devices' handles may wait to be revoked.
pub(crate) const QUEUE: usize = 64;
/// How many revocations run at once.
pub(crate) const CONCURRENCY: usize = 4;

/// Where `Auth` sends an unpaired device's push relay handles.
pub(crate) type Revocations = mpsc::Sender<Vec<Handle>>;

/// The queue [`queue`] fills and [`run`] drains.
pub(crate) fn channel() -> (Revocations, mpsc::Receiver<Vec<Handle>>) {
    mpsc::channel(QUEUE)
}

/// Queues one unpaired device's `handles` without waiting; when the queue
/// is full or [`run`] stopped, they are dropped.
pub(crate) fn queue(revocations: &Revocations, handles: Vec<Handle>) {
    if handles.is_empty() {
        return;
    }
    match revocations.try_send(handles) {
        Ok(()) => {}
        Err(TrySendError::Full(handles)) => tracing::warn!(
            handles = handles.len(),
            "too many push relay revocations waiting; these are left to expire"
        ),
        Err(TrySendError::Closed(handles)) => tracing::warn!(
            handles = handles.len(),
            "push relay revocations stopped; these are left to expire"
        ),
    }
}

/// Revokes each queued handle once with `revoke`, at most
/// [`CONCURRENCY`] at a time, until every sender is gone, handing each
/// call's result to `report` ([`log`] in the daemon) as soon as it ends,
/// whether or not more work arrives.
pub(crate) async fn run<F, A, R>(
    mut receiver: mpsc::Receiver<Vec<Handle>>,
    revoke: F,
    mut report: R,
) where
    F: Fn(Handle) -> A,
    A: Future<Output = RelayOutcome> + Send + 'static,
    R: FnMut(Result<RelayOutcome, JoinError>),
{
    let mut running = JoinSet::new();
    loop {
        tokio::select! {
            Some(result) = running.join_next(), if !running.is_empty() => report(result),
            received = receiver.recv() => {
                let Some(handles) = received else {
                    break;
                };
                for handle in handles {
                    while running.len() >= CONCURRENCY {
                        if let Some(result) = running.join_next().await {
                            report(result);
                        }
                    }
                    running.spawn(revoke(handle));
                }
            }
        }
    }
    while let Some(result) = running.join_next().await {
        report(result);
    }
}

/// Logs what came of one revocation, naming no handle or device.
pub(crate) fn log(result: Result<RelayOutcome, JoinError>) {
    match result {
        Ok(RelayOutcome::Delivered | RelayOutcome::Gone | RelayOutcome::Stale) => {
            // Revoked, or the push relay refuses the handle already.
            tracing::debug!("revoked a push relay handle");
        }
        Ok(outcome) => {
            tracing::info!(
                ?outcome,
                "could not revoke a push relay handle; it is left to expire"
            );
        }
        Err(error) => tracing::warn!(%error, "a push relay revocation failed"),
    }
}
