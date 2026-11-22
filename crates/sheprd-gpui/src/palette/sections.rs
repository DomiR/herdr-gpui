//! The Projects filter: the workspaces already open in Herdr under one
//! heading, then the configured projects without one under another. Projects
//! are looked for only once the filter is first shown, so no other filter
//! waits on the disk or searches them.

use super::{Action, Entry, Filter, Palette, PaletteConfig, Source, search};
use crate::{SheprdWindow, i18n::fl};
use std::{collections::HashSet, path::PathBuf};

/// A heading row of the Projects filter. Moving skips it and choosing it
/// does nothing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Section {
    Open,
    NotOpen,
}

impl Section {
    pub(super) fn entry(self) -> Entry {
        let label = match self {
            Self::Open => fl!("palette-section-open"),
            Self::NotOpen => fl!("palette-section-not-open"),
        };
        Entry::new(label, String::new(), "", Action::Section(self), None)
    }
}

/// Where the project scan stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Scan {
    /// Configured, and not looked for until the Projects filter is shown.
    Waiting,
    Loading,
    /// Found, or nothing configured to look for.
    Done,
}

impl Scan {
    pub(super) fn initial(config: &PaletteConfig, filter: Filter) -> Self {
        if !config.has_projects() {
            Self::Done
        } else if filter == Filter::Projects {
            Self::Loading
        } else {
            Self::Waiting
        }
    }
}

fn heading(entries: &[Entry], hit: &search::Hit) -> bool {
    matches!(entries[hit.index].action, Action::Section(_))
}

/// The Projects filter's rows: open workspaces, then projects and the row
/// that makes one, each group under its heading when it has rows. With no
/// query the most frecent projects come first, as their list has no order
/// of its own to keep.
pub(super) fn sectioned(
    entries: &[Entry],
    hits: Vec<search::Hit>,
    by_frecency: bool,
) -> Vec<search::Hit> {
    let (open, mut projects): (Vec<_>, Vec<_>) = hits
        .into_iter()
        .partition(|hit| matches!(entries[hit.index].action, Action::Go { .. }));
    if by_frecency {
        projects.sort_by_key(|hit| std::cmp::Reverse(entries[hit.index].boost));
    }
    let mut rows = Vec::with_capacity(open.len() + projects.len() + 2);
    for (section, group) in [(Section::Open, open), (Section::NotOpen, projects)] {
        if group.is_empty() {
            continue;
        }
        let index = entries
            .iter()
            .position(|entry| matches!(entry.action, Action::Section(s) if s == section));
        rows.extend(index.map(|index| search::Hit {
            index,
            depth: 0,
            highlights: search::Highlights::default(),
        }));
        rows.extend(group);
    }
    rows
}

/// The first row that can be chosen.
pub(super) fn first_row(entries: &[Entry], rows: &[search::Hit]) -> usize {
    rows.iter()
        .position(|hit| !heading(entries, hit))
        .unwrap_or(0)
}

/// The row one step from `from`, wrapping around and passing headings.
pub(super) fn step(entries: &[Entry], rows: &[search::Hit], from: usize, forward: bool) -> usize {
    let count = rows.len();
    let mut at = from;
    for _ in 0..count {
        at = if forward {
            (at + 1) % count
        } else {
            (at + count - 1) % count
        };
        if !heading(entries, &rows[at]) {
            return at;
        }
    }
    from
}

impl Palette {
    /// The rows listed, headings aside.
    pub(super) fn shown_rows(&self) -> usize {
        self.filtered
            .iter()
            .filter(|hit| !heading(&self.entries, hit))
            .count()
    }

    /// Every row the palette has, headings aside.
    pub(super) fn total_rows(&self) -> usize {
        self.entries
            .iter()
            .filter(|entry| !matches!(entry.action, Action::Section(_)))
            .count()
    }

    /// Whether a search outside the Projects filter should say that
    /// configured projects are not part of it.
    pub(super) fn searches_elsewhere(&self) -> bool {
        matches!(self.filter, Filter::All | Filter::Navigation)
            && !self.query.trim().is_empty()
            && self.configuration.has_projects()
    }
}

impl SheprdWindow {
    /// The folders of this computer's open workspaces, as their hosts report
    /// them. Compared as reported, without reading the disk: a project that
    /// slips through is still opened in its workspace when chosen.
    pub(super) fn open_project_roots(&self, sources: &[Source]) -> HashSet<PathBuf> {
        let herdr = sources
            .iter()
            .filter(|source| source.endpoint == crate::endpoint::LOCAL && source.enabled)
            .filter_map(|source| source.snapshot.as_deref())
            .flat_map(super::projects::workspace_roots)
            .map(|(_, root)| PathBuf::from(root));
        let kept = self
            .core_host()
            .and_then(|index| self.sessions(index).core)
            .into_iter()
            .flat_map(|core| {
                core.workspaces
                    .iter()
                    .map(|workspace| PathBuf::from(&workspace.cwd))
                    .collect::<Vec<_>>()
            });
        herdr.chain(kept).collect()
    }
}
