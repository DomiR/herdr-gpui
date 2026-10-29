//! Pairing a phone with the other machines this app connects to, so the
//! phone reaches each one's own remote control daemon while this computer
//! sleeps (`herdr_remote_control::control::hosts`).
//!
//! This app knows the SSH and cloud machines, and can run scripts on them;
//! its daemon cannot. So the daemon relays: the phone hands it one key per
//! host, and the supervisor takes each request and runs a job here. A job:
//! - copies this app's `herdr-remote-control` into the host's home when the
//!   host lacks this exact build (`crate::host_build`);
//! - runs `herdr-remote-control enroll` there with the phone's key, this
//!   computer's name, and this daemon's APNs key, which starts the host's
//!   daemon when it does not run and adds the phone;
//! - hands the host's answer (its fingerprint and addresses) back to this
//!   daemon, which the phone reads.
//!
//! Revoking the phone here owes each host a revocation, which a job carries
//! out with `herdr-remote-control revoke` once the host can be reached.
//! Everything here blocks; the supervisor runs it on the background
//! executor.

pub(crate) mod ask;

use crate::{
    automations::Machine,
    host_build::{self, Platform},
    teleport::Host,
};
use herdr_client::{ConnectTarget, ScriptLimits, shell_quote};
use herdr_remote_control::{
    app::enroll::{Apns, Input},
    control::{
        DeviceId,
        hosts::{self, Enroll, Enrolled, HostOutcome, HostPairing},
    },
};
use std::{sync::atomic::AtomicBool, time::Duration};

/// The daemon's name on a host, and the folder beside the app holding its
/// builds for other platforms.
const PROGRAM: &str = "herdr-remote-control";
const BUILDS: &str = "remote-control";
/// What `enroll` may print: one JSON answer.
const ENROLL_OUTPUT: u64 = 64 * 1024;
/// `enroll` may wait up to 20 s for a daemon it started.
const ENROLL_IDLE: Duration = Duration::from_secs(60);
/// A slow link can take a while to carry a few megabytes.
const COPY_IDLE: Duration = Duration::from_secs(300);
const COPY_OUTPUT: u64 = 64 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    // keep-sorted start
    #[error("The host's answer is not one this app can read")]
    Answer(#[source] serde_json::Error),
    #[error(transparent)]
    Build(#[from] host_build::Error),
    #[error("This app no longer connects to that host")]
    Gone,
    #[error("This app has no herdr-remote-control build for {platform}")]
    NoBuild { platform: String },
    #[error("Could not run herdr-remote-control on the host: {0}")]
    Run(#[source] herdr_client::Error),
    #[error("Remote control cannot be paired on this kind of host")]
    Unsupported,
    // keep-sorted end
}

/// A machine this app connects to: what the daemon calls it, and how to
/// reach it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Known {
    pub(crate) host: hosts::Host,
    pub(crate) target: ConnectTarget,
}

/// The machines a phone may be paired with: every enabled SSH and cloud
/// endpoint, once per machine however many sessions reach it, in the
/// window's order. This computer, WSL, and custom sockets are not hosts of
/// their own.
pub(crate) fn known<'a>(
    endpoints: impl IntoIterator<Item = (&'a str, &'a str, &'a ConnectTarget, bool)>,
) -> Vec<Known> {
    let mut machines = Vec::new();
    let mut known = Vec::new();
    for (id, label, target, enabled) in endpoints {
        let Some(machine) = Machine::of(target).filter(|machine| !machine.is_local()) else {
            continue;
        };
        if !enabled || machines.contains(&machine) {
            continue;
        }
        let host = hosts::Host {
            id: hosts::HostId(id.to_owned()),
            name: crate::notifications::safe_text(label, 64),
        };
        if !host.valid() {
            continue;
        }
        machines.push(machine);
        known.push(Known {
            host,
            target: target.clone(),
        });
    }
    known.truncate(hosts::MAX_HOSTS);
    known
}

/// One phone to pair with one host.
#[derive(Clone, Debug)]
pub(crate) struct Job {
    pub(crate) pairing: HostPairing,
    /// `None` when this app no longer connects to the host.
    pub(crate) target: Option<ConnectTarget>,
    /// This computer's name, kept beside the phone on the host.
    pub(crate) via: String,
    pub(crate) apns: Option<Apns>,
}

/// Runs `job`; how it ended, for the daemon to tell the phone.
pub(crate) fn pair(job: &Job, cancelled: &AtomicBool) -> HostOutcome {
    match enroll(job, cancelled) {
        Ok(enrolled) => HostOutcome::Paired(enrolled),
        Err(error) => {
            tracing::warn!(host = %job.pairing.host.0, %error, "Could not pair a phone with a host");
            HostOutcome::Failed {
                reason: reason(&error),
            }
        }
    }
}

fn enroll(job: &Job, cancelled: &AtomicBool) -> Result<Enrolled, Error> {
    let host = host(job.target.as_ref())?;
    let program = ensure_program(&host, cancelled)?;
    let input = Input {
        enroll: Enroll {
            name: job.pairing.name.clone(),
            public_key: job.pairing.public_key.clone(),
            via: job.via.clone(),
            replace: job.pairing.replace.clone(),
        },
        apns: job.apns.clone(),
    };
    let input = serde_json::to_vec(&input).map_err(Error::Answer)?;
    let mut output = Vec::new();
    host.stream(
        &format!("exec {program} enroll\n"),
        input.as_slice(),
        &mut output,
        ScriptLimits {
            output: ENROLL_OUTPUT,
            idle: ENROLL_IDLE,
        },
        cancelled,
    )
    .map_err(Error::Run)?;
    parse_enrolled(&output)
}

/// The host's answer to `enroll`, refused unless it is in bounds.
pub(crate) fn parse_enrolled(output: &[u8]) -> Result<Enrolled, Error> {
    let enrolled: Enrolled = serde_json::from_slice(output).map_err(Error::Answer)?;
    if !enrolled.valid() {
        return Err(Error::Answer(serde::de::Error::custom(
            "an answer out of bounds",
        )));
    }
    Ok(enrolled)
}

/// Revokes `device` on the host `target` reaches. A host with no
/// `herdr-remote-control` holds no device to revoke.
pub(crate) fn revoke(
    target: Option<&ConnectTarget>,
    device: &DeviceId,
    cancelled: &AtomicBool,
) -> Result<(), Error> {
    let host = host(target)?;
    host.capture(&revoke_script(device), cancelled)
        .map_err(Error::Run)?;
    Ok(())
}

pub(crate) fn revoke_script(device: &DeviceId) -> String {
    format!(
        "program=$(command -v {PROGRAM} || true)\n\
         [ -n \"$program\" ] || exit 0\n\
         exec \"$program\" revoke --device {}\n",
        shell_quote(&device.0)
    )
}

fn host(target: Option<&ConnectTarget>) -> Result<Host, Error> {
    let target = target.ok_or(Error::Gone)?;
    Host::new(target).map_err(|_| Error::Unsupported)
}

/// The version directory this app's build goes in on a host.
fn version() -> &'static str {
    crate::APP_VERSION
}

/// Copies this app's build to the host unless it holds the same bytes
/// already; the quoted path to run it by.
fn ensure_program(host: &Host, cancelled: &AtomicBool) -> Result<String, Error> {
    let probe = host
        .capture(&probe_script(version()), cancelled)
        .map_err(Error::Run)?;
    let probe = String::from_utf8_lossy(&probe);
    let mut lines = probe.lines();
    let platform = Platform::parse(lines.next().unwrap_or_default())?;
    let installed = lines.next().unwrap_or_default().trim();
    let bundled = crate::bundled::locate(super::service::PROGRAM);
    let source = host_build::build_for(
        &platform,
        &host_build::beside_app(),
        BUILDS,
        PROGRAM,
        bundled,
    )
    .ok_or_else(|| Error::NoBuild {
        platform: platform.to_string(),
    })?;
    let (_, sha256) = host_build::digest(PROGRAM, &source)?;
    if installed != sha256 {
        let file =
            std::fs::File::open(&source).map_err(|source_error| host_build::Error::Binary {
                program: PROGRAM,
                path: source.clone(),
                source: source_error,
            })?;
        host.stream(
            &host_build::copy_script(PROGRAM, version(), &sha256),
            file,
            Vec::new(),
            ScriptLimits {
                output: COPY_OUTPUT,
                idle: COPY_IDLE,
            },
            cancelled,
        )
        .map_err(|error| match &error {
            herdr_client::Error::ScriptExit { status, .. }
                if status.code() == Some(host_build::CHECKSUM_EXIT) =>
            {
                Error::Build(host_build::Error::Checksum)
            }
            _ => Error::Run(error),
        })?;
    }
    Ok(installed_path(version()))
}

/// The host's platform on the first line, then the SHA-256 of the build of
/// `version` it holds, or nothing.
pub(crate) fn probe_script(version: &str) -> String {
    format!(
        "uname -sm\n\
         file={}\n\
         if [ -f \"$file\" ]; then\n\
         if command -v sha256sum >/dev/null 2>&1; then sum=$(sha256sum \"$file\"); else sum=$(shasum -a 256 \"$file\"); fi\n\
         echo \"${{sum%% *}}\"\n\
         fi\n",
        installed_path(version)
    )
}

/// Where [`host_build::copy_script`] puts `version`, as a shell word.
fn installed_path(version: &str) -> String {
    format!(
        "\"$HOME/.local/share/{PROGRAM}/bin/\"{}\"/{PROGRAM}\"",
        shell_quote(version)
    )
}

/// `error` and its causes, for the phone to show.
fn reason(error: &Error) -> String {
    let mut text = error.to_string();
    let mut source = std::error::Error::source(error);
    while let Some(cause) = source {
        let said = cause.to_string();
        if !text.contains(&said) {
            text.push_str(": ");
            text.push_str(&said);
        }
        source = cause.source();
    }
    crate::notifications::safe_text(&text, hosts::MAX_REASON)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests;
