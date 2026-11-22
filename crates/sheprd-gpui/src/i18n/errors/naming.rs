//! Why an agent gave a rename dialog no name, from `error-naming.ftl`.

use super::Localize;
use crate::{i18n::fl, naming::Error};

impl Localize for Error {
    fn localize(&self) -> String {
        match self {
            Self::Blocked { agent } => fl!("error-naming-blocked", agent = agent.as_str()),
            Self::Chat(error) => error.localize(),
            Self::Claude(error) => error.localize(),
            Self::Core(error) => fl!("error-naming-core", error = error.to_string()),
            Self::Empty => fl!("error-naming-empty"),
            Self::Gone => fl!("error-naming-gone"),
            Self::Host(error) => error.localize(),
            Self::NoName { agent } => fl!("error-naming-no-name", agent = agent.as_str()),
            Self::Script(error) => fl!("error-naming-script", error = error.to_string()),
            Self::TimedOut { agent } => fl!("error-naming-timed-out", agent = agent.as_str()),
        }
    }
}
