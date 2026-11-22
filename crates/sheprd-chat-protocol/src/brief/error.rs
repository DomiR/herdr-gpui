use crate::EntryId;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{} briefs are already kept", super::MAX_BRIEFS)]
    Limit,
    #[error("entry {0:?} is not a prompt")]
    NotPrompt(EntryId),
}
