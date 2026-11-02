//! Version numbers agents name their changes against. Every change to a
//! document, from any writer, takes the next number; the document
//! remembers where its recent versions are in its history, to show an agent
//! whose number is stale exactly what changed since.

use crate::Origin;
use loro::Frontiers;
use std::{collections::VecDeque, fmt, str::FromStr};

/// How many recent versions a document can still show a diff from.
pub(crate) const KEPT: usize = 512;
/// The largest diff a conflict carries.
pub const MAX_DIFF: usize = 64 * 1024;

/// A document's version number: 1 when opened, one more for each change.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Version(pub u64);

impl Version {
    pub const FIRST: Self = Self(1);

    pub fn next(self) -> Self {
        Self(self.0.saturating_add(1))
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl FromStr for Version {
    type Err = crate::Error;

    fn from_str(text: &str) -> crate::Result<Self> {
        text.trim()
            .trim_start_matches('v')
            .parse()
            .map(Self)
            .map_err(|_| crate::Error::BadVersion)
    }
}

/// A command's version did not match the document's.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Conflict {
    pub expected: Version,
    pub current: Version,
    /// A unified diff from the expected version's text to the current one;
    /// `None` when that version is too old, unknown, or the diff too large.
    pub diff: Option<String>,
}

/// One kept version: where it is in the CRDT history, and who made it
/// (`None` for the text as opened).
#[derive(Debug)]
struct Kept {
    version: Version,
    frontiers: Frontiers,
    by: Option<Origin>,
}

/// Recent versions and where each is in the CRDT history, newest last.
#[derive(Debug, Default)]
pub(crate) struct History {
    kept: VecDeque<Kept>,
}

impl History {
    pub(crate) fn record(&mut self, version: Version, frontiers: Frontiers, by: Option<Origin>) {
        if self.kept.back().is_some_and(|last| last.version == version) {
            return;
        }
        if self.kept.len() == KEPT {
            self.kept.pop_front();
        }
        self.kept.push_back(Kept {
            version,
            frontiers,
            by,
        });
    }

    /// Where `version` last was.
    pub(crate) fn find(&self, version: Version) -> Option<&Frontiers> {
        self.kept
            .iter()
            .rev()
            .find(|kept| kept.version == version)
            .map(|kept| &kept.frontiers)
    }

    /// Who made the versions after `version`, each once, in order; `None`
    /// when `version` is no longer kept.
    pub(crate) fn origins_since(&self, version: Version) -> Option<Vec<Origin>> {
        let at = self.kept.iter().position(|kept| kept.version == version)?;
        let mut out: Vec<Origin> = Vec::new();
        for kept in self.kept.iter().skip(at + 1) {
            if let Some(by) = &kept.by
                && !out.contains(by)
            {
                out.push(by.clone());
            }
        }
        Some(out)
    }
}

/// A unified diff from `old` to `new`, labelled with their versions, or
/// `None` when it is over [`MAX_DIFF`].
pub(crate) fn unified(old: &str, new: &str, from: Version, to: Version) -> Option<String> {
    let diff = similar::TextDiff::from_lines(old, new);
    let text = diff
        .unified_diff()
        .context_radius(2)
        .header(&from.to_string(), &to.to_string())
        .to_string();
    (text.len() <= MAX_DIFF).then_some(text)
}
