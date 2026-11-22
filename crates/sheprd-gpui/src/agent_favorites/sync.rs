//! Meeting a device's own daemon, which keeps its favorite agents
//! (`sheprd_remote_control::control::favorites`). This computer's is one
//! socket away; another machine's is asked over SSH, as `device_tags` asks
//! for its tags. Everything here blocks: run it on the background executor.

use super::Error;
use crate::{device_tags::Daemon, remote, teleport::Host};
use sheprd_client::ScriptLimits;
use sheprd_remote_control::{
    app::favorites,
    control::favorites::{Change, Favorites},
};
use std::{sync::atomic::AtomicBool, time::Duration};

/// What `favorites` may print: one JSON answer of at most
/// `MAX_FAVORITES` panes.
const OUTPUT: u64 = 128 * 1024;
const IDLE: Duration = Duration::from_secs(20);

/// Finds the program without installing it: a favorite is no reason to put
/// a daemon on a machine. An older build refuses `favorites` and prints
/// nothing, as does a machine without the program. Standard input is drained
/// so the sender is never left writing to no one.
const READ: &str = "\
program=$(command -v sheprd-remote-control || command -v herdr-remote-control || true)
[ -n \"$program\" ] || program=\"$HOME/.local/bin/sheprd-remote-control\"
if [ -x \"$program\" ]; then \"$program\" favorites 2>/dev/null || true; fi
cat >/dev/null
";
/// [`READ`], making the change given on standard input.
const CHANGE: &str = "\
program=$(command -v sheprd-remote-control || command -v herdr-remote-control || true)
[ -n \"$program\" ] || program=\"$HOME/.local/bin/sheprd-remote-control\"
if [ -x \"$program\" ]; then \"$program\" favorites --offer 2>/dev/null || true; fi
cat >/dev/null
";

/// Makes `change`, if any, on the device's daemon and returns the favorites
/// that stand, or `None` when the device has no daemon that keeps them.
pub(super) fn exchange(
    daemon: &Daemon,
    change: Option<Change>,
    cancelled: &AtomicBool,
) -> Result<Option<Favorites>, Error> {
    match daemon {
        Daemon::Local => {
            let Ok(state_dir) = remote::state_dir() else {
                return Ok(None);
            };
            Ok(Some(favorites::exchange(&state_dir, change)?))
        }
        Daemon::Remote(target) => {
            let host = Host::new(target).map_err(|_| Error::Unsupported)?;
            let (script, input) = match &change {
                Some(change) => (CHANGE, serde_json::to_vec(change).map_err(Error::Answer)?),
                None => (READ, Vec::new()),
            };
            let mut output = Vec::new();
            host.stream(
                script,
                input.as_slice(),
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
    }
}

/// The host's answer, bounded; `None` when it printed nothing.
pub(super) fn parse(output: &[u8]) -> Result<Option<Favorites>, Error> {
    if output.iter().all(u8::is_ascii_whitespace) {
        return Ok(None);
    }
    let answer: Favorites = serde_json::from_slice(output).map_err(Error::Answer)?;
    Ok(Some(answer.bounded()))
}
