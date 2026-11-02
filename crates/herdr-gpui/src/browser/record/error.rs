//! Why a page recording could not start, finish, or become a bundle.

use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    // keep-sorted start
    #[error("Scrawl did not build the review: {detail}")]
    Import { detail: String },
    #[error("Scrawl took longer than {} minutes to build the review", .0.as_secs() / 60)]
    ImportTimeout(std::time::Duration),
    #[error("Scrawl wrote no summary in {}", path.display())]
    MissingSummary {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[cfg(target_os = "macos")]
    #[error("This window is not one macOS will record; is it on screen?")]
    NoWindow,
    #[cfg(target_os = "macos")]
    #[error("The page is not on screen")]
    PageHidden,
    #[cfg(target_os = "macos")]
    #[error("The recording did not finish: {0}")]
    RecordingFailed(String),
    #[cfg(target_os = "macos")]
    #[error("The recording did not finish in time")]
    RecordingTimeout,
    #[cfg(target_os = "macos")]
    #[error("Recording needs macOS 15 or later")]
    RecordingUnavailable,
    #[cfg(target_os = "macos")]
    #[error("Install Scrawl to record page reviews: scrawl.kitlangton.com")]
    ScrawlMissing,
    #[cfg(target_os = "macos")]
    #[error(
        "Allow Herdr to record the screen in System Settings, Privacy & Security, Screen & System Audio Recording, then open Herdr again"
    )]
    ScreenRecording(#[source] screencapturekit::error::SCError),
    #[cfg(target_os = "macos")]
    #[error("macOS refused to record: {0}")]
    Stream(#[source] screencapturekit::error::SCError),
    #[cfg(target_os = "macos")]
    #[error("The recording's file cannot be written there")]
    VideoPath(#[source] screencapturekit::recording_output::InvalidOutputPath),
    // keep-sorted end
}
