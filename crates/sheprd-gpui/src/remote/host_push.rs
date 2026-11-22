//! How each linked host's daemon sends iPhone pushes, for `remote
//! push-status` and Settings beside this computer's own. A host is asked
//! over SSH, as `remote::links` enrolls a phone there, by running its
//! `sheprd-remote-control push-status`, which asks the daemon running
//! there. Nothing is installed to ask: a host without the program, or with
//! one too old to know the verb, prints nothing and shows as unknown.
//! Everything here blocks: run it on the background executor.

mod error;

use crate::teleport::Host;
pub(crate) use error::Error;
use sheprd_client::{ConnectTarget, ScriptLimits};
use sheprd_control_protocol::protocol::remote::HostPush;
use sheprd_remote_control::control::push::PushStatus;
use std::{sync::atomic::AtomicBool, time::Duration};

/// What `push-status` may print: one small JSON answer.
const OUTPUT: u64 = 16 * 1024;
/// How long a host may say nothing; the caller waits for every host at
/// once (`RemoteOp::wait_seconds`).
const IDLE: Duration = Duration::from_secs(15);

/// Finds the program as `device_tags` does, without installing it.
pub(crate) const SCRIPT: &str = "\
program=$(command -v sheprd-remote-control || command -v herdr-remote-control || true)
[ -n \"$program\" ] || program=\"$HOME/.local/bin/sheprd-remote-control\"
if [ -x \"$program\" ]; then \"$program\" push-status 2>/dev/null || true; fi
";

/// Asks the host `target` reaches; `None` when no daemon there answered.
pub(crate) fn ask(
    target: &ConnectTarget,
    cancelled: &AtomicBool,
) -> Result<Option<PushStatus>, Error> {
    let host = Host::new(target).map_err(|_| Error::Unsupported)?;
    let mut output = Vec::new();
    host.stream(
        SCRIPT,
        &[][..],
        &mut output,
        ScriptLimits {
            output: OUTPUT,
            idle: IDLE,
        },
        cancelled,
    )
    .map_err(Error::Run)?;
    parse(&output)
}

/// The host's answer; `None` when it printed nothing.
pub(crate) fn parse(output: &[u8]) -> Result<Option<PushStatus>, Error> {
    if output.iter().all(u8::is_ascii_whitespace) {
        return Ok(None);
    }
    serde_json::from_slice(output)
        .map(Some)
        .map_err(Error::Answer)
}

/// The reply's line for the host named `name`. The error becomes its
/// display text here, at the reply's edge.
pub(crate) fn answer(name: String, asked: Result<Option<PushStatus>, Error>) -> HostPush {
    match asked {
        Ok(status) => HostPush {
            name,
            status,
            error: None,
        },
        Err(error) => HostPush {
            name,
            status: None,
            error: Some(error.to_string()),
        },
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests;
