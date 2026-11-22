//! The `sheprd-acp` chats run on the machines that get a host build
//! (`host.rs`). Both targets' builds travel in one archive with their own
//! signed manifest (schema 5) packed first (`packed.rs`).

use super::{packed::Pack, *};

pub(super) const CHATS: Pack = Pack {
    schema: 5,
    manifest: ("host-chat-manifest.json", "host-chat-manifest.sig"),
    program: "sheprd-acp",
    suffix: "-linux-musl.tar.gz",
    targets: &HOST_TARGETS,
};

/// Downloads this release's `sheprd-acp` built for `target` into
/// `destination`, checked against the signed manifest in its archive by the
/// key this build embeds; a local build has neither and refuses. On error
/// the caller removes `destination`. Blocking: run it on a worker thread.
pub fn download_host_chat(target: &str, destination: &Path, cancel: &AtomicBool) -> Result<()> {
    let (version, key) =
        host::release_identity(crate::APP_VERSION, option_env!("HERDR_UPDATE_PUBLIC_KEY"))?;
    CHATS.download(version, key, target, destination, cancel)
}
