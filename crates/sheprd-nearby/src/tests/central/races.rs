//! Answers that arrive late or out of turn: reads from computers that left
//! or from an earlier connection, discovery the radio refused, refreshes
//! due while messages keep coming, and discovery changes applied in order.

use super::*;
use crate::central::{REFRESH, Refresh, latest::Latest};
use std::time::{Duration, Instant};

/// Discovered, connected, and resolved; returns its `ReadInfo`'s attempt.
fn resolved(central: &mut Central) -> u64 {
    central.input(Input::Discovered(DEVICE.into()));
    central.input(Input::Connected(DEVICE.into()));
    let steps = central.input(Input::Resolved {
        device: DEVICE.into(),
        info: true,
        pair: true,
        chunk: 20,
    });
    let [Step::Act(Action::ReadInfo { attempt, .. })] = steps[..] else {
        panic!("no read: {steps:?}");
    };
    attempt
}

fn named(attempt: u64) -> Input {
    Input::Info {
        device: DEVICE.into(),
        attempt,
        value: Some(info("Studio")),
    }
}

#[test]
fn a_read_answered_after_the_computer_left_lists_nothing() {
    let mut central = scanning();
    let attempt = resolved(&mut central);
    central.input(Input::Disconnected(DEVICE.into()));
    assert_eq!(central.input(named(attempt)), []);
}

#[test]
fn a_read_from_an_earlier_connection_is_told_apart() {
    let mut central = scanning();
    let first = resolved(&mut central);
    central.input(Input::Disconnected(DEVICE.into()));
    let second = resolved(&mut central);
    assert_ne!(first, second);
    assert_eq!(central.input(named(first)), []);
    let id = central.id(DEVICE);
    assert_eq!(
        central.input(named(second)),
        [emit(Event::Found {
            peripheral: id,
            name: "Studio".into(),
            fp: "ABCD".into(),
        })]
    );
    // Refreshes read with the current connection's number.
    assert_eq!(
        central.tick(),
        [act(Action::ReadInfo {
            device: DEVICE.into(),
            attempt: second,
        })]
    );
}

#[test]
fn refused_discovery_is_asked_for_again_at_the_next_refresh() {
    let mut central = scanning();
    let steps = central.input(Input::DiscoveryFailed {
        on: true,
        failure: "busy".into(),
    });
    assert!(
        matches!(&steps[..], [Step::Emit(Event::Error { message })] if message == "discovery: busy"),
        "{steps:?}"
    );
    assert_eq!(central.tick(), [act(Action::Discover(true))]);
    assert_eq!(central.tick(), []);
}

#[test]
fn a_refusal_of_an_earlier_ask_changes_nothing() {
    let mut central = scanning();
    assert_eq!(
        central.command(Command::Scan(false)),
        [act(Action::Discover(false))]
    );
    // The start asked before the stop is refused only now.
    central.input(Input::DiscoveryFailed {
        on: true,
        failure: "busy".into(),
    });
    // Discovery is off, as last asked, so scanning again starts it.
    assert_eq!(
        central.command(Command::Scan(true)),
        [
            emit(Event::CentralState {
                state: Radio::PoweredOn
            }),
            act(Action::Discover(true)),
        ]
    );
}

#[test]
fn a_refresh_due_while_messages_keep_coming_still_runs() {
    let mut refresh = Refresh::default();
    let start = Instant::now();
    // Scanning starts: the first refresh is a full period away.
    assert!(!refresh.due(true, start));
    assert_eq!(refresh.wait(start), REFRESH);
    assert!(!refresh.due(true, start + REFRESH / 2));
    // A message handled exactly at the deadline, or after it, is followed
    // by the refresh rather than pushing it back.
    assert!(refresh.due(true, start + REFRESH));
    assert!(!refresh.due(true, start + REFRESH + Duration::from_millis(1)));
    assert!(refresh.due(true, start + REFRESH * 3));
    // Not scanning: no refresh, and the loop sleeps long.
    assert!(!refresh.due(false, start + REFRESH * 4));
    assert_eq!(refresh.wait(start), Duration::from_secs(3600));
}

#[test]
fn discovery_changes_end_on_the_last_one_asked() {
    let latest = Latest::default();
    latest.put(true);
    latest.put(false);
    latest.put(true);
    // A worker busy while three were asked applies only the last.
    assert_eq!(latest.take(), Some(true));
    latest.put(false);
    latest.close();
    // What was asked before closing is still applied, then the worker ends.
    assert_eq!(latest.take(), Some(false));
    assert_eq!(latest.take(), None);
}

#[test]
fn a_waiting_worker_takes_changes_in_the_order_asked() {
    let latest = Arc::new(Latest::default());
    let (taken, applied) = mpsc::channel();
    let worker = {
        let latest = latest.clone();
        std::thread::spawn(move || {
            while let Some(on) = latest.take() {
                taken.send(on).unwrap();
            }
        })
    };
    latest.put(false);
    assert!(!applied.recv().unwrap());
    latest.put(true);
    assert!(applied.recv().unwrap());
    latest.close();
    worker.join().unwrap();
}
