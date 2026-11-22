//! Why no `sheprd-relay-agent` could be found for a plain host.
use crate::host_build;
use std::{io, path::PathBuf};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    // keep-sorted start
    #[error("Could not make a folder for the relay agent's download in {}: {source}", path.display())]
    Cache {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("Could not download this release's sheprd-relay-agent for {target}: {source}")]
    Download {
        target: String,
        #[source]
        source: sheprd_updater::Error,
    },
    #[error(transparent)]
    Host(#[from] host_build::Error),
    #[error("This development build ships no sheprd-relay-agent for {platform}")]
    NoBuild { platform: String },
    #[error("Could not find this app's state directory")]
    NoStateDir,
    #[error("A plain host runs Linux or macOS on x86_64 or ARM64; this host reports {platform}")]
    Unsupported { platform: String },
    // keep-sorted end
}
