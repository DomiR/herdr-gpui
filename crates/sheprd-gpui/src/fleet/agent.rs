//! A plain host's relay agent, over the app's SSH (`docs/relay-plan.md`,
//! Fleet, Spreading a host, step 3): putting `sheprd-relay-agent` on the
//! host as `host_build` puts this app's other helpers there, keeping it
//! running as a user service, reading its `status --json`, and handing it
//! the access file (`fleet apply`). Everything here blocks: run it on the
//! background executor.
//!
//! The build is the release's own for the host's platform, checked against
//! its signed manifest, or a development build's (`build`). It lands in
//! `~/.local/share/sheprd-relay-agent/bin/VERSION/`, linked from
//! `~/.local/bin`, and is copied only when the host holds other bytes; the
//! host checks the copy's SHA-256 before it is put in place.

pub(crate) mod build;

use super::Error;
use crate::{host_build, remote::relay_token::ListenToken, teleport::Host};
use sheprd_client::{ScriptLimits, shell_quote};
use sheprd_relay_agent::status::{RelayState, Report};
use sheprd_remote_control::control::fleet::{HostId, HostIntro, HostKind, SelfRecord, Verified};
use std::{sync::atomic::AtomicBool, time::Duration};

/// The agent's program name on the host.
pub(crate) const PROGRAM: &str = "sheprd-relay-agent";
/// The service's name: a systemd user unit, or a launchd agent's label.
const UNIT: &str = "sheprd-relay-agent.service";
const LABEL: &str = "org.sheprd.relay-agent";
/// A slow link can take a while to carry a few megabytes.
const COPY_IDLE: Duration = Duration::from_secs(300);
/// What a script on the host may print: the agent's `status --json` at
/// its largest, which every script here ends with.
const OUTPUT: u64 = sheprd_relay_agent::status::MAX_REPORT_BYTES as u64;
/// Starting the service and waiting for it to listen.
const SERVICE_IDLE: Duration = Duration::from_secs(60);
/// The most relays the service is started with; a record holds 16.
const MAX_RELAYS: usize = 16;
/// The most SSH host keys a record carries.
const MAX_HOST_KEYS: usize = 4;

/// A relay endpoint safe to write into a unit, a property list, and a
/// command line as it is: an `http(s)://` URL of plain characters.
pub(crate) fn plain_relay(relay: &str) -> bool {
    (relay.starts_with("https://") || relay.starts_with("http://"))
        && relay.len() <= 512
        && relay
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b":/.-_[]".contains(&byte))
}

/// A relay the agent listens on, with this computer's listen token for it
/// when it is a private relay.
#[derive(Clone, Debug)]
pub(crate) struct Relay {
    /// Its endpoint, as [`plain_relay`] allows.
    pub(crate) url: String,
    pub(crate) token: Option<ListenToken>,
}

/// Where the service's token file for the relay at `index` is.
fn token_file(index: usize) -> String {
    format!("$state/tokens/relay-{index}")
}

/// What the service script reads on its input: each token, in the order
/// of `relays`, one a line. Tokens never go in the script, which the
/// host's other users can see in its process list.
pub(crate) fn service_input(relays: &[Relay]) -> Vec<u8> {
    relays
        .iter()
        .filter_map(|relay| relay.token.as_ref())
        .flat_map(|token| [token.expose().as_bytes(), b"\n".as_slice()])
        .flatten()
        .copied()
        .collect()
}

/// Sets `port` to the port the host's SSH server took this session on,
/// the last field of `SSH_CONNECTION`, which the server sets from its own
/// end of the connection: where the agent forwards sessions to. 22 when
/// the session does not say, as for a login that is not over SSH.
pub(crate) const SSH_PORT: &str = r#"port=22
case "${SSH_CONNECTION:-}" in
*' '*)
  said=${SSH_CONNECTION##* }
  case "$said" in ''|*[!0-9]*) ;; *) [ "$said" -ge 1 ] && [ "$said" -le 65535 ] && port=$said ;; esac
  ;;
esac
"#;

/// Sets `domain` to the launchd domain the agent's job goes in on a Mac:
/// the account's graphical session when it has one, else its user domain,
/// which an account reached only over SSH has too. A `gui/` domain exists
/// only while the user is logged in at the screen, so an SSH-only Mac
/// would never start a job sent there.
pub(crate) const LAUNCHD_DOMAIN: &str = r#"uid=$(id -u)
if launchctl print "gui/$uid" >/dev/null 2>&1; then
  domain="gui/$uid"
else
  domain="user/$uid"
fi
"#;

/// The host script that keeps the agent running as a user service on
/// `relays`: each private relay's token in a private file it passes with
/// `--token-file`, then a systemd user unit on Linux, a launchd agent on
/// macOS, both restarted so an update takes effect. It then waits up to
/// twenty seconds for a relay to let the agent listen, and prints its
/// `status --json`. The tokens come on its input ([`service_input`]).
pub(crate) fn service_script(relays: &[Relay]) -> String {
    let token = |index: usize, relay: &Relay| relay.token.as_ref().map(|_| token_file(index));
    let args: String = relays
        .iter()
        .enumerate()
        .map(|(index, relay)| match token(index, relay) {
            Some(file) => format!(" --relay {} --token-file \\\"{file}\\\"", relay.url),
            None => format!(" --relay {}", relay.url),
        })
        .collect();
    let plist_args: String = relays
        .iter()
        .enumerate()
        .map(|(index, relay)| {
            let token = token(index, relay).map_or_else(String::new, |file| {
                format!("<string>--token-file</string><string>{file}</string>")
            });
            format!(
                "<string>--relay</string><string>{}</string>{token}",
                relay.url
            )
        })
        .collect();
    let save_tokens: String = relays
        .iter()
        .enumerate()
        .filter_map(|(index, relay)| token(index, relay))
        .map(|file| {
            format!("  IFS= read -r token || exit 6\n  printf '%s\\n' \"$token\" > \"{file}\"\n")
        })
        .collect();
    format!(
        // i18n: data
        r#"agent="$HOME/.local/bin/{PROGRAM}"
# The port this host's SSH server listens on, which the agent forwards to.
{SSH_PORT}# The state directory every SSH command of this account uses, as the agent
# picks it by default, given to the service explicitly: its manager may not
# share this login's environment (`XDG_STATE_HOME`).
if [ "$(uname -s)" = Darwin ]; then
  state="$HOME/Library/Application Support/{PROGRAM}"
else
  case "${{XDG_STATE_HOME:-}}" in
  /*) state="$XDG_STATE_HOME/{PROGRAM}" ;;
  *) state="$HOME/.local/state/{PROGRAM}" ;;
  esac
fi
case "$state" in *'&'*|*'<'*|*'>'*|*'"'*|*'%'*|*'\'*) echo "the state directory's name cannot go in a service" >&2; exit 5;; esac
case "$HOME" in *'&'*|*'<'*|*'>'*|*'"'*|*'%'*) echo "the home directory's name cannot go in a service" >&2; exit 5;; esac
# Private relays' listen tokens, readable by this account alone; a relay
# no longer private leaves no file behind.
mkdir -p "$state/tokens"
chmod 700 "$state" "$state/tokens"
rm -f "$state/tokens/"relay-*
(
  umask 077
{save_tokens}) || {{ echo "a relay's listen token did not arrive" >&2; exit 6; }}
case "$(uname -s)" in
Darwin)
  plist="$HOME/Library/LaunchAgents/{LABEL}.plist"
  mkdir -p "$HOME/Library/LaunchAgents"
  printf '%s\n' '<?xml version="1.0" encoding="UTF-8"?>' \
    '<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">' \
    '<plist version="1.0"><dict>' \
    '<key>Label</key><string>{LABEL}</string>' \
    "<key>ProgramArguments</key><array><string>$agent</string><string>run</string><string>--state-dir</string><string>$state</string><string>--forward</string><string>$port</string>{plist_args}</array>" \
    '<key>RunAtLoad</key><true/><key>KeepAlive</key><true/>' \
    '<key>LimitLoadToSessionType</key><array><string>Aqua</string><string>Background</string></array>' \
    '</dict></plist>' > "$plist.part"
{LAUNCHD_DOMAIN}  job="$domain/{LABEL}"
  # A job an earlier share left in the other domain goes: one agent runs.
  for other in "gui/$uid" "user/$uid"; do
    [ "$other" = "$domain" ] || launchctl bootout "$other/{LABEL}" >/dev/null 2>&1 || true
  done
  if cmp -s "$plist.part" "$plist" && launchctl print "$job" >/dev/null 2>&1; then
    # The same job is loaded: restart it in place, so the new build runs
    # and the agent is never unloaded.
    rm -f "$plist.part"
    launchctl kickstart -k "$job" >&2
  else
    mv -f "$plist.part" "$plist"
    launchctl bootout "$job" >/dev/null 2>&1 || true
    # `bootout` can return before the old job is gone, and a `bootstrap`
    # sent until then fails: try again for a while rather than leave no
    # service.
    tries=0
    until launchctl bootstrap "$domain" "$plist" 2>/dev/null; do
      tries=$((tries + 1))
      if [ "$tries" -ge 10 ]; then launchctl bootstrap "$domain" "$plist" >&2; break; fi
      sleep 1
    done
  fi
  ;;
*)
  dir="${{XDG_CONFIG_HOME:-$HOME/.config}}/systemd/user"
  mkdir -p "$dir"
  printf '%s\n' '[Unit]' 'Description=sheprd-relay-agent: Sheprd reaches this SSH server through relays' \
    '' '[Service]' "ExecStart=\"$agent\" run --state-dir \"$state\" --forward $port{args}" 'Restart=on-failure' 'RestartSec=10' \
    '' '[Install]' 'WantedBy=default.target' > "$dir/{UNIT}.part"
  mv -f "$dir/{UNIT}.part" "$dir/{UNIT}"
  # Without lingering a user unit stops at logout; allowed for oneself on
  # most systems, and harmless where it is not.
  loginctl enable-linger "$(id -un)" >/dev/null 2>&1 || true
  systemctl --user daemon-reload >&2
  systemctl --user enable {UNIT} >&2
  systemctl --user restart {UNIT} >&2
  ;;
esac
i=0
while [ $i -lt 20 ]; do
  case "$("$agent" status --json --state-dir "$state" 2>/dev/null)" in *'"state": "listening"'*) break;; esac
  sleep 1
  i=$((i + 1))
done
"$agent" status --json --state-dir "$state"
"#
    )
}

/// Runs the agent [`install`] put on the host, the one its service runs,
/// with `ARGS`, or prints nothing when there is none. Never another
/// `sheprd-relay-agent` on `PATH`: an older one there would be handed
/// commands it does not know, or a fleet the service never sees.
const RUN: &str = "\
agent=\"$HOME/.local/bin/sheprd-relay-agent\"
if [ -x \"$agent\" ]; then \"$agent\" ARGS; fi
";

fn run(host: &Host, args: &str, input: &[u8], cancelled: &AtomicBool) -> Result<Vec<u8>, Error> {
    let mut output = Vec::new();
    host.stream(
        &RUN.replace("ARGS", args),
        input,
        &mut output,
        ScriptLimits {
            output: OUTPUT,
            idle: Host::QUERY_IDLE,
        },
        cancelled,
    )
    .map_err(Error::Run)?;
    Ok(output)
}

/// The agent's `status --json`, or `None` when the host has none.
pub(super) fn status(host: &Host, cancelled: &AtomicBool) -> Result<Option<Report>, Error> {
    let output = run(host, "status --json", b"", cancelled)?;
    parse_status(&output)
}

/// `status --json` as the agent printed it; nothing printed, no agent.
pub(crate) fn parse_status(output: &[u8]) -> Result<Option<Report>, Error> {
    if output.iter().all(u8::is_ascii_whitespace) {
        return Ok(None);
    }
    Ok(Some(Report::parse(output)?))
}

/// Hands the agent `access`, the file the daemon computed; what it holds
/// after.
pub(super) fn apply(host: &Host, access: &str, cancelled: &AtomicBool) -> Result<Report, Error> {
    let output = run(host, "fleet apply", access.as_bytes(), cancelled)?;
    parse_status(&output)?.ok_or(Error::NoProgram)
}

/// Has the agent forget its fleet (`fleet scrub`): its access file and
/// the fleet's `authorized_keys` lines go, so no member reaches the host
/// through it any more.
pub(super) fn scrub(host: &Host, cancelled: &AtomicBool) -> Result<(), Error> {
    run(host, "fleet scrub", b"", cancelled)?;
    Ok(())
}

/// Puts this app's build of the agent on `host` unless the same bytes are
/// there, keeps it running on `relays`, and reads its status. Blocks.
pub(super) fn install(
    host: &Host,
    relays: &[Relay],
    cancelled: &AtomicBool,
) -> Result<Report, Error> {
    let uname = host.capture("uname -sm\n", cancelled).map_err(Error::Run)?;
    // Refused before anything is copied: a platform with no agent.
    let platform = build::platform(&String::from_utf8_lossy(&uname))?;
    let located = build::locate(&platform, cancelled)?;
    let source = located.path();
    let (_, sha256) = host_build::digest(PROGRAM, source)?;
    let installed = host
        .capture(&probe_script(crate::APP_VERSION), cancelled)
        .map_err(Error::Run)?;
    if String::from_utf8_lossy(&installed).trim() != sha256 {
        let file =
            std::fs::File::open(source).map_err(|source_error| host_build::Error::Binary {
                program: PROGRAM,
                path: source.to_owned(),
                source: source_error,
            })?;
        host.stream(
            &host_build::copy_script(PROGRAM, crate::APP_VERSION, &sha256),
            file,
            Vec::new(),
            ScriptLimits {
                output: OUTPUT,
                idle: COPY_IDLE,
            },
            cancelled,
        )
        .map_err(|error| match &error {
            sheprd_client::Error::ScriptExit { status, .. }
                if status.code() == Some(host_build::CHECKSUM_EXIT) =>
            {
                Error::Build(host_build::Error::Checksum)
            }
            _ => Error::Run(error),
        })?;
    }
    let mut output = Vec::new();
    host.stream(
        &service_script(relays),
        &service_input(relays)[..],
        &mut output,
        ScriptLimits {
            output: OUTPUT,
            idle: SERVICE_IDLE,
        },
        cancelled,
    )
    .map_err(Error::Run)?;
    running(parse_status(&output)?.ok_or(Error::NoProgram)?)
}

/// `report`, once the service it came from runs and a relay lets it
/// listen: a host whose agent never started, or that every relay refused
/// (a private relay's token missing or wrong), would be shared under a
/// route nothing listens on.
pub(crate) fn running(report: Report) -> Result<Report, Error> {
    if !report.running {
        return Err(Error::NotRunning);
    }
    if !report
        .relays
        .values()
        .any(|relay| *relay == RelayState::Listening)
    {
        return Err(Error::NotListening);
    }
    Ok(report)
}

/// Prints the SHA-256 of the agent of `version` the host holds, or nothing.
fn probe_script(version: &str) -> String {
    format!(
        // i18n: data
        "file=\"$HOME/.local/share/{PROGRAM}/bin/\"{}\"/{PROGRAM}\"\n\
         if [ -f \"$file\" ]; then\n\
         if command -v sha256sum >/dev/null 2>&1; then sum=$(sha256sum \"$file\"); else sum=$(shasum -a 256 \"$file\"); fi\n\
         echo \"${{sum%% *}}\"\n\
         fi\n",
        shell_quote(version)
    )
}

/// The account the host's SSH session logs in as.
pub(super) fn user(host: &Host, cancelled: &AtomicBool) -> Result<String, Error> {
    let output = host.capture("id -un\n", cancelled).map_err(Error::Run)?;
    let user = String::from_utf8_lossy(&output).trim().to_owned();
    let valid = !user.is_empty()
        && user.len() <= 64
        && !user.starts_with('-')
        && user
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte));
    valid.then_some(user).ok_or(Error::NoUser)
}

/// The host keys the agent reported that this computer's known hosts list
/// for the host, in the agent's order: a strictly checked session would
/// accept each. The first names the host in the fleet.
pub(crate) fn trusted_keys(reported: &[String], known: &[String]) -> Vec<String> {
    reported
        .iter()
        .filter(|key| known.contains(key))
        .take(MAX_HOST_KEYS)
        .cloned()
        .collect()
}

/// The plain host the fleet names a machine whose agent reported
/// `reported`, when this computer trusts its first key.
pub(crate) fn host_id(reported: &[String], known: &[String]) -> Option<HostId> {
    let first = reported.first()?;
    known.contains(first).then(|| HostId::plain(first))
}

/// The intro a member shares a plain host with: what its agent reported,
/// held to this computer's known hosts, logged in as `user`, on `relays`,
/// vouched for at `now_ms`.
pub(crate) fn intro(
    report: &Report,
    known: &[String],
    name: &str,
    user: &str,
    relays: &[String],
    now_ms: u64,
) -> Result<HostIntro, Error> {
    // The first key names the host: it must be one the session trusted.
    if host_id(&report.ssh_host_keys, known).is_none() {
        return Err(Error::HostKey);
    }
    let ssh_host_keys = trusted_keys(&report.ssh_host_keys, known);
    let name: String = name
        .chars()
        .filter(|char| !char.is_control())
        .take(64)
        .collect();
    let mut intro = HostIntro {
        kind: HostKind::Agent,
        name: if name.trim().is_empty() {
            "host".to_owned() // i18n: data
        } else {
            name
        },
        user: user.to_owned(),
        ca: None,
        record: SelfRecord {
            host: HostId([0; 32]),
            rev: now_ms,
            route: report.route.clone(),
            relays: relays.iter().take(MAX_RELAYS).cloned().collect(),
            identity: report.identity.clone(),
            direct: Vec::new(),
            ssh_host_keys,
            signed_ms: now_ms,
        },
        record_signature: None,
        verified: Verified::KnownHosts,
    };
    intro.record.host = intro.id().map_err(Error::Intro)?;
    // Checks every field as the daemon will.
    intro.to_bytes().map_err(Error::Intro)?;
    Ok(intro)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests;
