//! Which `sheprd-relay-agent` a plain host gets (`docs/relay-plan.md`,
//! Fleet, Spreading a host, step 3), from its `uname -sm`:
//!
//! - a release copies only the release's own build for the host's
//!   platform: static musl on Linux, Developer ID signed on each Mac
//!   architecture. It downloads the release's agent archive once per
//!   share into a private folder and uses the build only after checking
//!   it against the Ed25519-signed manifest packed first in the archive
//!   (`sheprd_updater::download_host_agent`), by the key this build
//!   embeds, each time it shares a host. Nothing else reaches the host: not a program on `PATH`, not a
//!   file beside the app. The host then checks the copy's SHA-256 before
//!   it puts it in place (`host_build::copy_script`), and only then runs it.
//! - a development build has no signed release to read: it copies the
//!   agent beside its executable to a host of its kind, else
//!   `relay-agent/TRIPLE/` there, never one from `PATH`.
//!
//! A host on any other platform (Windows, the BSDs, 32-bit Linux) is
//! refused before anything is copied: the agent's service and installer
//! are POSIX shell with systemd or launchd.

mod error;

pub use error::Error;

use super::PROGRAM;
use crate::host_build::{self, Platform};
use std::{
    path::{Path, PathBuf},
    sync::atomic::AtomicBool,
};

/// Where builds for other platforms ship, beside a development build's
/// executable.
pub(super) const BUILDS: &str = "relay-agent";

/// The platform `uname -sm` names, when the release ships an agent for it.
pub(crate) fn platform(uname: &str) -> Result<Platform, Error> {
    let platform = Platform::parse(uname).map_err(|error| match error {
        host_build::Error::Platform(platform) => Error::Unsupported { platform },
        other => Error::from(other),
    })?;
    target(&platform).ok_or_else(|| Error::Unsupported {
        platform: platform.to_string(),
    })?;
    Ok(platform)
}

/// The release target of the agent built for `platform`.
pub(crate) fn target(platform: &Platform) -> Option<&'static str> {
    // `triple` names every system but macOS as Linux.
    if !matches!(platform.os, "linux" | "macos") {
        return None;
    }
    let triple = platform.triple();
    sheprd_updater::AGENT_TARGETS
        .into_iter()
        .find(|target| *target == triple)
}

/// Where the build for a host comes from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Source {
    /// The release's build for this target, checked against its signed
    /// manifest.
    Release(&'static str),
    /// A development build's own agent, or one shipped beside it.
    Local(PathBuf),
}

/// Which build fits a host on `platform`: a release's own, when this is a
/// release (`release`), else this development build's (`bundled` for a
/// host of its kind, else `relay-agent/TRIPLE/` in `beside`).
pub(crate) fn choose(
    platform: &Platform,
    release: bool,
    bundled: Option<PathBuf>,
    beside: &Path,
) -> Result<Source, Error> {
    let unsupported = || Error::Unsupported {
        platform: platform.to_string(),
    };
    let target = target(platform).ok_or_else(unsupported)?;
    if release {
        return Ok(Source::Release(target));
    }
    build_for(platform, beside, bundled)
        .map(Source::Local)
        .ok_or_else(|| Error::NoBuild {
            platform: platform.to_string(),
        })
}

/// A development build's agent for `platform`: `bundled`, its own, when the
/// host is this kind of computer, else `relay-agent/TRIPLE/` in `beside`.
pub(crate) fn build_for(
    platform: &Platform,
    beside: &Path,
    bundled: Option<PathBuf>,
) -> Option<PathBuf> {
    host_build::build_for(platform, beside, BUILDS, PROGRAM, bundled)
}

/// A build of the agent on this computer, ready to copy.
#[derive(Debug)]
pub(crate) enum Located {
    /// The release's build, checked against its signed manifest just now,
    /// in a private folder removed with it.
    Checked {
        _dir: tempfile::TempDir,
        path: PathBuf,
    },
    /// A development build's.
    Local(PathBuf),
}

impl Located {
    pub(crate) fn path(&self) -> &Path {
        match self {
            Self::Checked { path, .. } | Self::Local(path) => path,
        }
    }
}

/// The build of the agent for a host on `platform`, on this computer.
/// Blocks: a release downloads it.
pub(super) fn locate(platform: &Platform, cancelled: &AtomicBool) -> Result<Located, Error> {
    // Only what sits beside this app's executable: never a program on
    // `PATH`, which anything in the login environment could put first.
    let exe = std::env::current_exe().ok();
    let bundled = (!crate::RELEASE_BUILD)
        .then(|| crate::bundled::locate_in(PROGRAM, exe.as_deref(), None))
        .flatten();
    match choose(
        platform,
        crate::RELEASE_BUILD,
        bundled,
        &host_build::beside_app(),
    )? {
        Source::Release(target) => download(target, cancelled),
        Source::Local(path) => Ok(Located::Local(path)),
    }
}

/// The release's build for `target`, downloaded and checked against the
/// signed manifest into a new private folder in this app's state. Nothing
/// is kept for the next share, so no file left on this computer is ever
/// taken for a checked build. Blocks.
fn download(target: &'static str, cancelled: &AtomicBool) -> Result<Located, Error> {
    let state = crate::remote::state_dir().map_err(|_| Error::NoStateDir)?;
    let failed = |source| Error::Cache {
        path: state.clone(),
        source,
    };
    std::fs::create_dir_all(&state).map_err(failed)?;
    let dir = tempfile::Builder::new()
        .prefix(".sheprd-relay-agent-")
        .tempdir_in(&state)
        .map_err(failed)?;
    let path = dir.path().join(PROGRAM);
    sheprd_updater::download_host_agent(target, &path, cancelled).map_err(|source| {
        Error::Download {
            target: target.to_owned(),
            source,
        }
    })?;
    Ok(Located::Checked { _dir: dir, path })
}
