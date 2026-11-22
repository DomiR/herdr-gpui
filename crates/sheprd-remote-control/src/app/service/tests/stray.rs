//! A daemon the service manager did not start, such as an older release's
//! or one started by hand, holds the port until it is asked to exit: a
//! restart or an update stops whatever answers before starting the service,
//! or the new daemon fails to bind on every retry while the old one serves.
//! The wait for it to go is bounded in time, not in tries.

use super::*;
use std::time::{Duration, Instant};

fn on() -> RemoteConfig {
    RemoteConfig {
        enabled: true,
        ..RemoteConfig::default()
    }
}

fn launchd(dir: &Path) -> Host {
    Host {
        manager: Manager::LaunchAgent { uid: 501 },
        file: dir.join("LaunchAgents/so.pen.herdr-gpui.remote.plist"),
        release: Some("20261120.1".into()),
        ..host(dir)
    }
}

/// What happened, in order: requests to the daemon, the wait for it to
/// exit, and manager commands.
#[derive(Default)]
struct Log(RefCell<Vec<String>>);

impl Log {
    fn push(&self, entry: impl Into<String>) {
        self.0.borrow_mut().push(entry.into());
    }

    fn entries(self) -> Vec<String> {
        self.0.into_inner()
    }
}

/// A daemon of `version` that takes a shutdown when `agrees`.
fn daemon<'a>(
    log: &'a Log,
    version: &'a str,
    agrees: bool,
) -> impl Fn(&Request) -> Result<Response, ControlError> + 'a {
    move |request| match request {
        Request::Shutdown => {
            log.push("shutdown");
            if agrees {
                Ok(Response::Done)
            } else {
                Ok(status(version))
            }
        }
        _ => Ok(status(version)),
    }
}

#[test]
fn an_update_stops_the_old_daemon_and_waits_before_restarting_the_service() {
    let dir = tempfile::tempdir().unwrap();
    let host = launchd(dir.path());
    let no_core = || Err(sheprd_core_client::Error::NoHome);
    reconcile_with_core(&on(), &host, |_| unreachable(), |_| Ok(()), || {}, no_core).unwrap();
    let log = Log::default();
    let outcome = reconcile_with_core(
        &on(),
        &host,
        daemon(&log, "20261106.1", true),
        |step| {
            log.push(command(step));
            Ok(())
        },
        || log.push("exited"),
        no_core,
    )
    .unwrap();
    assert_eq!(outcome, Outcome::Started);
    let entries = log.entries();
    assert_eq!(entries[..2], ["shutdown", "exited"], "{entries:?}");
    assert_eq!(
        entries.last().map(String::as_str),
        Some("launchctl kickstart -k gui/501/so.pen.herdr-gpui.remote")
    );
}

#[test]
fn a_restart_stops_whatever_daemon_answers_first() {
    let dir = tempfile::tempdir().unwrap();
    let host = launchd(dir.path());
    reconcile(&on(), &host, |_| unreachable(), |_| Ok(())).unwrap();
    let log = Log::default();
    restart_with_wait(
        &on(),
        &host,
        daemon(&log, "20261120.1", true),
        |step| {
            log.push(command(step));
            Ok(())
        },
        || log.push("exited"),
    )
    .unwrap();
    let entries = log.entries();
    assert_eq!(entries[..2], ["shutdown", "exited"], "{entries:?}");
    assert!(
        entries.iter().any(|entry| entry.contains("kickstart -k")),
        "{entries:?}"
    );
}

#[test]
fn a_daemon_that_refuses_the_shutdown_is_not_waited_for() {
    let dir = tempfile::tempdir().unwrap();
    let host = launchd(dir.path());
    reconcile(&on(), &host, |_| unreachable(), |_| Ok(())).unwrap();
    let log = Log::default();
    restart_with_wait(
        &on(),
        &host,
        daemon(&log, "20261120.1", false),
        |step| {
            log.push(command(step));
            Ok(())
        },
        || panic!("nothing agreed to exit"),
    )
    .unwrap();
    assert!(
        log.entries()
            .iter()
            .any(|entry| entry.contains("kickstart"))
    );
}

#[test]
fn nothing_answering_restarts_without_waiting() {
    let dir = tempfile::tempdir().unwrap();
    let host = launchd(dir.path());
    let ran = RefCell::new(Vec::new());
    restart_with_wait(
        &on(),
        &host,
        |_| unreachable(),
        |step| {
            ran.borrow_mut().push(command(step));
            Ok(())
        },
        || panic!("nothing to wait for"),
    )
    .unwrap();
    assert!(
        ran.into_inner()
            .iter()
            .any(|entry| entry.contains("bootstrap")),
        "a missing definition is written and loaded"
    );
}

/// A clock the probes and sleeps advance, for the wait's own tests.
struct Clock {
    start: Instant,
    elapsed: RefCell<Duration>,
}

impl Clock {
    fn new() -> Self {
        Self {
            start: Instant::now(),
            elapsed: RefCell::new(Duration::ZERO),
        }
    }

    fn now(&self) -> Instant {
        self.start + *self.elapsed.borrow()
    }

    fn pass(&self, by: Duration) {
        *self.elapsed.borrow_mut() += by;
    }
}

#[test]
fn the_wait_ends_once_the_daemon_stops_answering() {
    let clock = Clock::new();
    let probes = RefCell::new(0);
    wait_for_exit(
        |_| {
            *probes.borrow_mut() += 1;
            *probes.borrow() < 3
        },
        || clock.now(),
        |by| clock.pass(by),
    );
    assert_eq!(probes.into_inner(), 3);
    assert_eq!(*clock.elapsed.borrow(), SETTLE_DELAY * 2);
}

#[test]
fn a_daemon_that_keeps_answering_is_waited_for_the_budget_and_no_longer() {
    let clock = Clock::new();
    let limits = RefCell::new(Vec::new());
    wait_for_exit(
        |limit| {
            limits.borrow_mut().push(limit);
            true
        },
        || clock.now(),
        |by| clock.pass(by),
    );
    assert_eq!(*clock.elapsed.borrow(), EXIT_BUDGET);
    assert!(
        limits
            .into_inner()
            .iter()
            .all(|limit| *limit <= SETTLE_DELAY)
    );
}

#[test]
fn slow_probes_count_against_the_budget() {
    // Each probe takes all the time it is given, as a daemon that accepts
    // no connection while it exits would make it.
    let clock = Clock::new();
    let left = RefCell::new(Vec::new());
    wait_for_exit(
        |limit| {
            left.borrow_mut()
                .push(EXIT_BUDGET.saturating_sub(*clock.elapsed.borrow()));
            assert!(
                limit <= *left.borrow().last().unwrap(),
                "a probe outlasts the budget"
            );
            clock.pass(limit);
            true
        },
        || clock.now(),
        |by| clock.pass(by),
    );
    assert_eq!(*clock.elapsed.borrow(), EXIT_BUDGET);
}
