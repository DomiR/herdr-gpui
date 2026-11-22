//! The brief `sheprd chat new --prompt` reads: `error-chat-brief.ftl`.

use super::Localize;
use crate::{
    control::{brief::Error, protocol::chat::MAX_BRIEF},
    i18n::fl,
};

impl Localize for Error {
    fn localize(&self) -> String {
        match self {
            // keep-sorted start
            Self::Empty => fl!("error-chat-brief-empty"),
            Self::Open { path, .. } => {
                fl!("error-chat-brief-open", path = path.display().to_string())
            }
            Self::Read(_) => fl!("error-chat-brief-read"),
            Self::Text => fl!("error-chat-brief-text"),
            Self::TooLong => fl!("error-chat-brief-too-long", bytes = MAX_BRIEF),
            // keep-sorted end
        }
    }
}
