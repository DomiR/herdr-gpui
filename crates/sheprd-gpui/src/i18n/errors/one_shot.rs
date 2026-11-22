//! Why Claude, run once on its own, gave no answer, from `error-one-shot.ftl`.

use super::Localize;
use crate::i18n::fl;
use sheprd_client::one_shot::Error;

impl Localize for Error {
    fn localize(&self) -> String {
        match self {
            Self::Cancelled => fl!("error-one-shot-cancelled"),
            Self::Empty => fl!("error-one-shot-empty"),
            Self::Failed => fl!("error-one-shot-failed"),
            Self::NoAgent => fl!("error-one-shot-no-agent"),
            Self::Start(_) => fl!("error-one-shot-start"),
            Self::TimedOut => fl!("error-one-shot-timed-out"),
        }
    }
}
