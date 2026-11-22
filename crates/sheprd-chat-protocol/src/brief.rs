//! Prompts another agent wrote, such as the brief an agent starts a chat
//! with (`sheprd chat new --prompt`). The person did not type them, so a
//! client may show them as a document to read rather than as their words.
//!
//! The mark lives beside the entries, not in [`EntryKind::User`], so a
//! client that predates it reads the prompt as an ordinary one.

mod error;

pub use error::Error;

use serde::{Deserialize, Serialize};

use crate::{Agent, EntryId, EntryKind, Transcript};

/// The most briefs a transcript keeps; a new one past it is refused.
pub const MAX_BRIEFS: usize = 16;

/// A user entry another agent wrote.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Brief {
    pub entry: EntryId,
    /// The agent that wrote it, when it ran in a chat Sheprd knows.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<Agent>,
}

impl Transcript {
    /// The brief `id` is, if another agent wrote it.
    pub fn brief(&self, id: EntryId) -> Option<&Brief> {
        self.briefs.iter().find(|brief| brief.entry == id)
    }

    /// Marks a user entry as a brief; marking one again replaces its sender.
    pub(crate) fn add_brief(&mut self, brief: Brief) -> Result<(), Error> {
        if !matches!(
            self.entry(brief.entry).map(|entry| &entry.kind),
            Some(EntryKind::User { .. })
        ) {
            return Err(Error::NotPrompt(brief.entry));
        }
        if let Some(known) = self
            .briefs
            .iter_mut()
            .find(|known| known.entry == brief.entry)
        {
            *known = brief;
            return Ok(());
        }
        if self.briefs.len() >= MAX_BRIEFS {
            return Err(Error::Limit);
        }
        self.briefs.push(brief);
        Ok(())
    }

    /// Forgets the briefs of entries dropped from the front.
    pub(crate) fn drop_briefs_before(&mut self, before: EntryId) {
        self.briefs.retain(|brief| brief.entry >= before);
    }
}
