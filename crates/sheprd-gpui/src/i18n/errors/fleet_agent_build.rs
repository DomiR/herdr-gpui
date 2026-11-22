//! A plain host's relay agent build: `error-fleet-agent-build.ftl`.

use super::Localize;
use crate::{fleet::agent::build::Error, i18n::fl};

impl Localize for Error {
    fn localize(&self) -> String {
        match self {
            // keep-sorted start
            Self::Cache { path, source } => fl!(
                "error-fleet-agent-build-cache",
                path = path.display().to_string(),
                error = source.to_string()
            ),
            Self::Download { target, source } => fl!(
                "error-fleet-agent-build-download",
                target = target.as_str(),
                error = source.localize()
            ),
            Self::Host(error) => error.localize(),
            Self::NoBuild { platform } => fl!(
                "error-fleet-agent-build-no-build",
                platform = platform.as_str()
            ),
            Self::NoStateDir => fl!("error-fleet-agent-build-no-state-dir"),
            Self::Unsupported { platform } => fl!(
                "error-fleet-agent-build-unsupported",
                platform = platform.as_str()
            ),
            // keep-sorted end
        }
    }
}
