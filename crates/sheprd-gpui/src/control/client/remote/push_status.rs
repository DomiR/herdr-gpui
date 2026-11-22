//! `remote push-status` as text: how iPhone pushes leave this computer,
//! and how the last alert and Live Activity push went. English, as all
//! command line output is.

use sheprd_control_protocol::protocol::remote::HostPush;
use sheprd_remote_control::control::push::{Last, Mode, Outcome, PushStatus};

pub(super) fn text(
    enabled: bool,
    push_relay: bool,
    status: Option<&PushStatus>,
    error: Option<&str>,
) -> String {
    let setting = if push_relay { "on" } else { "off" };
    let Some(status) = status else {
        return match (enabled, error) {
            (false, _) => format!("Remote control is off; the push relay is {setting} in Settings"),
            // No answer does not mean the daemon is down: one too old to
            // know the request closes the connection too.
            (true, Some(error)) => format!("Push status is unavailable: {error}"),
            (true, None) => "Push status is unavailable".to_owned(),
        };
    };
    let now = crate::remote::now_ms();
    [
        format!("iPhone pushes: {}", mode(status.mode)),
        format!("Push relay setting: {setting}"),
        format!("Last alert: {}", last(status.alert.as_ref(), now)),
        format!(
            "Last Live Activity push: {}",
            last(status.live_activity.as_ref(), now)
        ),
    ]
    .join("\n")
}

/// One line per linked host: how its daemon sends pushes, and how its last
/// alert went, or why it said nothing.
pub(super) fn hosts(hosts: &[HostPush]) -> String {
    let now = crate::remote::now_ms();
    hosts
        .iter()
        .map(|host| {
            let name = crate::notifications::safe_text(&host.name, 64);
            match (&host.status, &host.error) {
                (Some(status), _) => format!(
                    "  {name}: {}; last alert: {}",
                    host_mode(status.mode),
                    last(status.alert.as_ref(), now)
                ),
                (None, Some(error)) => format!("  {name}: unavailable: {error}"),
                (None, None) => format!(
                    "  {name}: unknown (no remote control daemon runs there, or it is too old to say)"
                ),
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// A host's mode, whose push relay setting this computer does not know.
fn host_mode(mode: Mode) -> &'static str {
    match mode {
        Mode::Off => "off (the push relay is off)",
        Mode::PushRelay => "through the push relay",
        // An older daemon still sending with its own APNs key says
        // `own_key`, which this Sheprd reads as a mode it does not know.
        Mode::Other => "a mode this Sheprd does not know",
    }
}

fn mode(mode: Mode) -> &'static str {
    match mode {
        Mode::Off => "off (the push relay is off)",
        Mode::PushRelay => "through the push relay (push.sheprd.org)",
        // An older daemon still sending with its own APNs key says
        // `own_key`, which this Sheprd reads as a mode it does not know.
        Mode::Other => "a mode this Sheprd does not know; restart remote control, or update Sheprd",
    }
}

fn last(last: Option<&Last>, now_ms: u64) -> String {
    let Some(last) = last else {
        return "none since the daemon started".to_owned();
    };
    let ago = crate::ago::english(now_ms.saturating_sub(last.at_ms) / 1000);
    format!("{}, {ago}", outcome(&last.outcome))
}

fn outcome(outcome: &Outcome) -> String {
    match outcome {
        Outcome::Delivered => "delivered".to_owned(),
        Outcome::Gone => {
            "the phone's registration expired; it registers again when the app opens".to_owned()
        }
        Outcome::Limited => "held back: too many pushes for that phone".to_owned(),
        Outcome::Refused {
            status,
            reason: Some(reason),
        } => format!(
            "refused ({status}, {})",
            crate::notifications::safe_text(reason, 64)
        ),
        Outcome::Refused {
            status,
            reason: None,
        } => format!("refused ({status})"),
        Outcome::Unreachable => "could not reach the push relay".to_owned(),
        Outcome::Unsent => "not sent: it could not be sealed".to_owned(),
        Outcome::Other => "an outcome this Sheprd does not know".to_owned(),
    }
}
