use super::*;
use sheprd_remote_control::control::push::{Mode, PushStatus};
use std::sync::atomic::AtomicBool;

#[test]
fn a_push_status_poll_reports_only_what_changed() {
    let mut supervisor = Supervisor::default();
    let relay = || {
        Ok(Response::PushStatus(PushStatus {
            mode: Mode::PushRelay,
            ..PushStatus::default()
        }))
    };
    assert!(supervisor.push_polled(relay()));
    assert!(!supervisor.push_polled(relay()));
    assert_eq!(
        supervisor.push_status.as_ref().map(|status| status.mode),
        Some(Mode::PushRelay)
    );
    // A daemon too old to answer leaves it unknown rather than off.
    assert!(supervisor.push_polled(Err(control::Error::NoAnswer)));
    assert_eq!(supervisor.push_status, None);
    assert!(!supervisor.push_polled(Ok(Response::Done)));
}

static PUSH_DAEMON_DOWN: AtomicBool = AtomicBool::new(false);

fn push_daemon(_: &Path, request: &Request) -> Result<Response, control::Error> {
    if PUSH_DAEMON_DOWN.load(Ordering::SeqCst) {
        return Err(control::Error::NoAnswer);
    }
    match request {
        Request::Status => Ok(Response::Status(status(0))),
        Request::Devices => Ok(Response::Devices { devices: vec![] }),
        Request::PushStatus => Ok(Response::PushStatus(PushStatus {
            mode: Mode::PushRelay,
            ..PushStatus::default()
        })),
        _ => Ok(Response::Done),
    }
}

#[gpui::test]
fn a_daemon_that_stops_answering_forgets_its_push_status(cx: &mut TestAppContext) {
    cx.update(|cx| Supervisor::fixture(cx, on(), push_daemon));
    let _lease = cx.update(Supervisor::lease);
    cx.update(Supervisor::tick);
    cx.run_until_parked();
    cx.update(|cx| {
        assert_eq!(
            Supervisor::push_status(cx).map(|status| status.mode),
            Some(Mode::PushRelay)
        );
    });
    // Settings must not keep showing the last mode and dot once it is down.
    PUSH_DAEMON_DOWN.store(true, Ordering::SeqCst);
    cx.update(|cx| cx.global_mut::<Supervisor>().poke = true);
    cx.update(Supervisor::tick);
    cx.run_until_parked();
    cx.update(|cx| assert_eq!(Supervisor::push_status(cx), None));
}
