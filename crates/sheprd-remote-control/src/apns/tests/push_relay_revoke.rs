//! Revoking an unpaired device's push relay handles: each once, at most a
//! few at a time, never blocking the unpair, and never a token.

use super::*;
use crate::apns::push_relay_revoke::{self, CONCURRENCY, QUEUE};
use sheprd_apns::{RelayHost, RelayOutcome};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicUsize, Ordering},
};

fn handle(byte: char) -> Handle {
    Handle::parse(
        &format!("1a2b3c4d.{}", byte.to_string().repeat(160)),
        RelayHost::Sandbox,
    )
    .unwrap()
}

#[test]
fn a_registration_lists_each_push_relay_handle_once_and_no_token() {
    let registration = Registration {
        activities: vec![
            ActivityToken {
                pane: "w1-p1".into(),
                token: handle('A').into(),
            },
            ActivityToken {
                pane: "w1-p2".into(),
                token: token(1),
            },
            ActivityToken {
                pane: "w1-p3".into(),
                token: handle('B').into(),
            },
        ],
        alert: Some(handle('B').into()),
        push_to_start: Some(handle('C').into()),
        ..Registration::default()
    };
    assert_eq!(
        registration.handles(),
        vec![handle('C'), handle('B'), handle('A')]
    );
    let tokens = Registration {
        alert: Some(token(2)),
        push_to_start: Some(token(3)),
        ..Registration::default()
    };
    assert!(tokens.handles().is_empty());
}

#[tokio::test]
async fn each_queued_push_relay_handle_is_revoked_once() {
    let (revocations, receiver) = push_relay_revoke::channel();
    push_relay_revoke::queue(&revocations, vec![handle('A'), handle('B')]);
    push_relay_revoke::queue(&revocations, Vec::new());
    push_relay_revoke::queue(&revocations, vec![handle('C')]);
    drop(revocations);
    let revoked = Arc::new(Mutex::new(Vec::new()));
    let seen = revoked.clone();
    push_relay_revoke::run(
        receiver,
        move |handle| {
            let seen = seen.clone();
            async move {
                seen.lock().unwrap().push(handle);
                RelayOutcome::Delivered
            }
        },
        push_relay_revoke::log,
    )
    .await;
    assert_eq!(
        *revoked.lock().unwrap(),
        vec![handle('A'), handle('B'), handle('C')]
    );
}

#[tokio::test]
async fn at_most_a_few_push_relay_revocations_run_at_once() {
    let (revocations, receiver) = push_relay_revoke::channel();
    let handles: Vec<Handle> = ('A'..='L').map(handle).collect();
    push_relay_revoke::queue(&revocations, handles.clone());
    drop(revocations);
    let running = Arc::new(AtomicUsize::new(0));
    let most = Arc::new(AtomicUsize::new(0));
    let calls = Arc::new(AtomicUsize::new(0));
    let (running_in, most_in, calls_in) = (running.clone(), most.clone(), calls.clone());
    push_relay_revoke::run(
        receiver,
        move |_| {
            let (running, most, calls) = (running_in.clone(), most_in.clone(), calls_in.clone());
            async move {
                let now = running.fetch_add(1, Ordering::SeqCst) + 1;
                most.fetch_max(now, Ordering::SeqCst);
                for _ in 0..4 {
                    tokio::task::yield_now().await;
                }
                running.fetch_sub(1, Ordering::SeqCst);
                calls.fetch_add(1, Ordering::SeqCst);
                // An unreachable push relay is not tried again.
                RelayOutcome::Unavailable
            }
        },
        push_relay_revoke::log,
    )
    .await;
    assert_eq!(calls.load(Ordering::SeqCst), handles.len());
    assert!(most.load(Ordering::SeqCst) <= CONCURRENCY);
}

#[tokio::test]
async fn a_lone_failed_revocation_is_reported_while_the_queue_stays_open() {
    let (revocations, receiver) = push_relay_revoke::channel();
    let (reports, mut reported) = tokio::sync::mpsc::unbounded_channel();
    let runner = tokio::spawn(push_relay_revoke::run(
        receiver,
        |_| async { RelayOutcome::Unavailable },
        move |result| {
            let _ = reports.send(result.unwrap());
        },
    ));
    push_relay_revoke::queue(&revocations, vec![handle('A')]);
    // `Auth` keeps its sender for the daemon's life, so nothing else
    // arrives to wake the loop: the report must come on its own.
    let outcome = tokio::time::timeout(std::time::Duration::from_secs(5), reported.recv())
        .await
        .expect("a failed revocation is reported without more work arriving");
    assert_eq!(outcome, Some(RelayOutcome::Unavailable));
    assert!(!runner.is_finished());
    drop(revocations);
    runner.await.unwrap();
}

#[test]
fn a_full_or_stopped_queue_drops_push_relay_handles_without_waiting() {
    let (revocations, mut receiver) = push_relay_revoke::channel();
    for _ in 0..=QUEUE {
        push_relay_revoke::queue(&revocations, vec![handle('A')]);
    }
    let mut queued = 0;
    while receiver.try_recv().is_ok() {
        queued += 1;
    }
    assert_eq!(queued, QUEUE);
    drop(receiver);
    push_relay_revoke::queue(&revocations, vec![handle('A')]);
}

#[tokio::test]
async fn a_push_relay_that_is_out_is_not_called_to_revoke() {
    let relays = Relays::new().unwrap();
    let now = 1_000;
    relays.mark_down(RelayHost::Sandbox, now);
    // Out: given up at once, with no connection made.
    assert_eq!(
        relays.revoke(&handle('A'), now).await,
        RelayOutcome::Unavailable
    );
}
