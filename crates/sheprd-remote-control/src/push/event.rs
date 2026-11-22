//! A notification decision's id. The daemon makes one each time it decides
//! a change is worth a push, and every copy of that push carries it: to
//! each device, held while its owner is near, coalesced by the throttle,
//! or sent again another way. The phone's extension keeps the ids it showed
//! recently, so a push that arrives twice shows once.

use uuid::Uuid;

/// 32 lowercase hex digits, random.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct EventId(String);

impl EventId {
    pub(crate) fn new() -> Self {
        Self(Uuid::new_v4().simple().to_string())
    }

    /// A fixed id, for fixtures the phone's tests read.
    #[cfg(test)]
    pub(crate) fn fixed(id: &str) -> Self {
        Self(id.to_owned())
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}
