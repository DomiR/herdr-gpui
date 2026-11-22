//! A pairing link piped to `sheprd remote add-link`: `error-link-input.ftl`.

use super::Localize;
use crate::{control::link_input::Error, i18n::fl};

impl Localize for Error {
    fn localize(&self) -> String {
        match self {
            // keep-sorted start
            Self::Empty => fl!("error-link-input-empty"),
            Self::Link(error) => error.localize(),
            Self::Read(_) => fl!("error-link-input-read"),
            Self::Terminal => fl!("error-link-input-terminal"),
            Self::Text => fl!("error-link-input-text"),
            Self::TooLong => fl!("error-link-input-too-long"),
            // keep-sorted end
        }
    }
}
