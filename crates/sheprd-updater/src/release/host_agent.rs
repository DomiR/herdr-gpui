//! `sheprd-relay-agent`, which Sheprd installs over SSH on a machine it
//! shares as a plain host (`docs/relay-plan.md`, Fleet). Static musl builds
//! for Linux and Developer ID signed builds for each Mac architecture
//! travel in one archive with their own signed manifest (schema 6) packed
//! first (`packed.rs`), so a host of any of these platforms gets a build
//! checked against the release before it leaves this computer.

use super::{packed::Pack, *};

/// The platforms a release publishes `sheprd-relay-agent` for. Windows and
/// the BSDs are not among them: the agent's service, private files, and
/// installer are POSIX shell, systemd, and launchd.
pub const AGENT_TARGETS: [&str; 4] = [
    "x86_64-unknown-linux-musl",
    "aarch64-unknown-linux-musl",
    "x86_64-apple-darwin",
    "aarch64-apple-darwin",
];

pub(super) const AGENTS: Pack = Pack {
    schema: 6,
    manifest: ("host-agent-manifest.json", "host-agent-manifest.sig"),
    program: "sheprd-relay-agent",
    suffix: ".tar.gz",
    targets: &AGENT_TARGETS,
};

/// Downloads this release's `sheprd-relay-agent` built for `target` into
/// `destination`, checked against the signed manifest in its archive by
/// the key this build embeds; a local build has neither and refuses
/// ([`Error::LocalBuild`]). On error the caller removes `destination`.
/// Blocking: run it on a worker thread.
pub fn download_host_agent(target: &str, destination: &Path, cancel: &AtomicBool) -> Result<()> {
    let (version, key) =
        host::release_identity(crate::APP_VERSION, option_env!("HERDR_UPDATE_PUBLIC_KEY"))?;
    AGENTS.download(version, key, target, destination, cancel)
}
