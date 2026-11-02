//! Recording a page review: the page and the user's voice as a video, the
//! pen strokes drawn over it, and the elements under each stroke, built into
//! a Scrawl bundle the agent reads. Herdr UI records; Scrawl
//! (scrawl.kitlangton.com) transcribes the narration, picks the keyframes,
//! and writes the contact sheet and `summary.md`, as it does for a ScrawlKit
//! recording made inside an iPhone app. The timeline handed to it is in
//! ScrawlKit's format (`timeline`), and the bundle comes back as a note in
//! the page's notes panel, sent with the others.

mod error;
mod scrawl;
#[cfg(target_os = "macos")]
mod screen;
mod timeline;

pub(crate) use error::Error;
#[cfg(target_os = "macos")]
pub(crate) use scrawl::Bundle;
pub(crate) use scrawl::{bundle_dir, find as find_scrawl, import};
#[cfg(target_os = "macos")]
pub(crate) use screen::{Capture, Target};
pub(crate) use timeline::Session;

/// Where a recording's video and timeline wait for Scrawl: the app's private
/// state folder. Removed once Scrawl has built its bundle from them.
#[cfg(target_os = "macos")]
pub(crate) fn work_dir() -> crate::Result<std::path::PathBuf> {
    let dir = crate::preferences::state_dir()
        .ok_or(crate::Error::MissingStateRoot)?
        .join("recordings");
    std::fs::create_dir_all(&dir)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700))?;
    }
    Ok(dir)
}

#[cfg(test)]
mod tests;
