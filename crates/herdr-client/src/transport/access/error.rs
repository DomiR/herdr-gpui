//! Why Windows refused the daemon's pipe, as shown in the disconnection
//! reason. Each names the step that fixes it.

use std::{io, path::PathBuf};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    // keep-sorted start
    #[error(
        "Windows refused the Herdr server's pipe. A server started from an administrator \
         terminal admits only administrator clients: stop it there and start `herdr server` \
         from an ordinary terminal, or restart it with `herdr server \
         --allow-unelevated-clients` (this closes its panes)."
    )]
    ElevatedServer {
        #[source]
        source: io::Error,
    },
    #[error(
        "Windows refused the Herdr server's pipe because Herdr UI runs at low integrity, \
         usually because its files carry a low integrity label. Run `icacls \"{}\" \
         /setintegritylevel (OI)(CI)M /T`, then restart Herdr UI.",
        folder.as_deref().map_or_else(|| "<Herdr UI folder>".into(), |folder| folder.display().to_string())
    )]
    LowIntegrity {
        /// The folder holding this executable, the one to relabel.
        folder: Option<PathBuf>,
        #[source]
        source: io::Error,
    },
    // keep-sorted end
}
