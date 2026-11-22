//! Linked hosts' push status for Settings: asked again once a minute
//! while Settings watches, whether or not this computer's daemon runs.

use super::*;
use crate::remote::supervisor::push_status::HostPushes;
use sheprd_control_protocol::protocol::remote::HostPush;

fn down(_: &Path, _: &Request) -> Result<Response, control::Error> {
    Err(control::Error::NoAnswer)
}

#[test]
fn hosts_are_due_until_asked_then_after_a_minute() {
    let now = Instant::now();
    let mut pushes = HostPushes::default();
    assert!(pushes.due(now));
    pushes.asking = true;
    assert!(!pushes.due(now), "one ask at a time");
    pushes.asking = false;
    pushes.asked = Some(now);
    assert!(!pushes.due(now + Duration::from_secs(59)));
    assert!(pushes.due(now + Duration::from_secs(60)));
}

/// Remote control is off here and its daemon does not answer, yet a tick
/// while Settings watches still refreshes the hosts: with none linked,
/// what was shown is cleared rather than left standing.
#[gpui::test]
fn a_watched_tick_refreshes_hosts_while_this_daemon_is_off(cx: &mut TestAppContext) {
    cx.update(|cx| {
        Supervisor::fixture(cx, RemoteConfig::default(), down);
        Supervisor::set_host_pushes(
            cx,
            vec![HostPush {
                name: "gone".into(),
                status: None,
                error: None,
            }],
        );
        let supervisor = cx.global_mut::<Supervisor>();
        assert!(!supervisor.config.enabled);
        supervisor.watchers.set(1);
        Supervisor::tick(cx);
        assert!(Supervisor::host_pushes(cx).is_empty());
    });
}
