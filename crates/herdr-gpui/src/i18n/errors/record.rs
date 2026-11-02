//! Page recording failures, from `error-record.ftl`. Scrawl's, the
//! system's, and ScreenCaptureKit's text stays as it is.

use super::Localize;
use crate::{browser::record::Error, i18n::fl};

impl Localize for Error {
    fn localize(&self) -> String {
        match self {
            Self::Import { detail } => fl!("error-record-import", error = detail.clone()),
            Self::ImportTimeout(limit) => {
                fl!(
                    "error-record-import-timeout",
                    minutes = limit.as_secs() / 60
                )
            }
            Self::MissingSummary { path, .. } => {
                fl!(
                    "error-record-missing-summary",
                    path = path.display().to_string()
                )
            }
            #[cfg(target_os = "macos")]
            Self::NoWindow => fl!("error-record-no-window"),
            #[cfg(target_os = "macos")]
            Self::PageHidden => fl!("error-record-page-hidden"),
            #[cfg(target_os = "macos")]
            Self::RecordingFailed(error) => fl!("error-record-failed", error = error.clone()),
            #[cfg(target_os = "macos")]
            Self::RecordingTimeout => fl!("error-record-timeout"),
            #[cfg(target_os = "macos")]
            Self::RecordingUnavailable => fl!("error-record-unavailable"),
            #[cfg(target_os = "macos")]
            Self::ScrawlMissing => fl!("error-record-scrawl-missing"),
            #[cfg(target_os = "macos")]
            Self::ScreenRecording(_) => fl!("error-record-screen-recording"),
            #[cfg(target_os = "macos")]
            Self::Stream(error) => fl!("error-record-stream", error = error.to_string()),
            #[cfg(target_os = "macos")]
            Self::VideoPath(_) => fl!("error-record-video-path"),
        }
    }
}
