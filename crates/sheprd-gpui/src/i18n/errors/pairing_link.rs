//! Why a pasted pairing link is refused, from `error-pairing-link.ftl`.

use super::Localize;
use crate::i18n::fl;
use sheprd_remote_control::control::pairing_link::Error;

impl Localize for Error {
    fn localize(&self) -> String {
        match self {
            // keep-sorted start
            Self::Format => fl!("error-pairing-link-format"),
            Self::NoRelays => fl!("error-pairing-link-no-relays"),
            Self::TooLong => fl!("error-pairing-link-too-long"),
            Self::Version => fl!("error-pairing-link-version"),
            // keep-sorted end
        }
    }
}
