//! File paths written as inline code, as agents write them in todo lists
//! (`` `src/main.rs` ``, `` `docs/plan.md:12` ``), made links when the file
//! exists, as comet's inline-code links do. Paths resolve against the
//! document's folder, then the root of the Git checkout it is in. Every
//! check of the file system runs off the UI thread and is remembered per
//! text, so drawing only reads answers already found.

use super::editor::DocEditor;
use gpui::{AppContext as _, Context};
use herdr_doc::markdown::{self, Line, Style};
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
};

/// The most code spans one check looks up.
const MAX_PROBES: usize = 512;
/// The longest code span taken for a path.
const MAX_PATH: usize = 512;

/// A path a code span names, and the line in it, if one follows `:`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct FileLink {
    pub(crate) path: PathBuf,
    pub(crate) line: Option<u32>,
}

/// What this view knows of the code spans it shows.
#[derive(Debug, Default)]
pub(super) struct FileLinks {
    /// Where relative paths resolve, nearest first; found off the UI thread.
    roots: Option<Vec<PathBuf>>,
    known: HashMap<String, Option<FileLink>>,
    probing: bool,
}

impl FileLinks {
    /// The file `span` names, if it is known to exist.
    pub(super) fn get(&self, span: &str) -> Option<&FileLink> {
        self.known.get(span)?.as_ref()
    }
}

/// Whether a code span's text could be a path: one word with a dot or a
/// slash, not an address.
fn looks_like_path(text: &str) -> bool {
    !text.is_empty()
        && text.len() <= MAX_PATH
        && (text.contains('/') || text.contains('.'))
        && !text.contains("://")
        && !text.chars().any(|ch| ch.is_whitespace() || ch.is_control())
        && !text.starts_with('-')
}

/// `text` split into a path and a line, as `path:12` names one.
fn path_and_line(text: &str) -> (&str, Option<u32>) {
    match text.rsplit_once(':') {
        Some((path, line)) if !path.is_empty() => match line.parse::<u32>() {
            Ok(line) if line > 0 => (path, Some(line)),
            _ => (text, None),
        },
        _ => (text, None),
    }
}

/// The code spans of `lines` that could be paths.
pub(super) fn candidates(text: &str, lines: &[Line]) -> Vec<String> {
    let mut out = Vec::new();
    for line in lines {
        for span in markdown::inline(text, line) {
            if span.style != Style::Code {
                continue;
            }
            let Some(code) = text.get(span.range) else {
                continue;
            };
            if looks_like_path(code) {
                out.push(code.to_owned());
            }
        }
    }
    out
}

/// The roots relative paths resolve against: the document's folder, then
/// the Git checkout it is in. Blocking.
fn roots(document: &Path) -> Vec<PathBuf> {
    let Some(folder) = document.parent() else {
        return Vec::new();
    };
    let mut roots = vec![folder.to_owned()];
    if let Some(checkout) = folder
        .ancestors()
        .find(|dir| dir.join(".git").exists())
        .filter(|checkout| *checkout != folder)
    {
        roots.push(checkout.to_owned());
    }
    roots
}

/// Looks `span` up under `roots`. Blocking.
fn probe(roots: &[PathBuf], span: &str) -> Option<FileLink> {
    let (path, line) = path_and_line(span);
    let path = Path::new(path);
    let candidates: Vec<PathBuf> = if path.is_absolute() {
        vec![path.to_owned()]
    } else {
        roots.iter().map(|root| root.join(path)).collect()
    };
    candidates
        .into_iter()
        .find(|candidate| candidate.is_file())
        .map(|path| FileLink { path, line })
}

impl DocEditor {
    /// Starts looking up the code spans no check has answered yet.
    pub(super) fn probe_links(&mut self, wanted: Vec<String>, cx: &mut Context<Self>) {
        if self.links.probing {
            return;
        }
        let unknown: Vec<String> = wanted
            .into_iter()
            .filter(|span| !self.links.known.contains_key(span))
            .collect::<HashSet<_>>()
            .into_iter()
            .take(MAX_PROBES)
            .collect();
        if unknown.is_empty() && self.links.roots.is_some() {
            return;
        }
        self.links.probing = true;
        let document = self.buffer.read(cx).path().to_owned();
        let known_roots = self.links.roots.clone();
        let job = cx.background_spawn(async move {
            let roots = known_roots.unwrap_or_else(|| roots(&document));
            let found: Vec<(String, Option<FileLink>)> = unknown
                .into_iter()
                .map(|span| {
                    let link = probe(&roots, &span);
                    (span, link)
                })
                .collect();
            (roots, found)
        });
        cx.spawn(async move |this, cx| {
            let (roots, found) = job.await;
            let _ = this.update(cx, |this, cx| {
                this.links.probing = false;
                this.links.roots = Some(roots);
                let any = found.iter().any(|(_, link)| link.is_some());
                this.links.known.extend(found);
                if any {
                    cx.notify();
                }
            });
        })
        .detach();
    }
}

#[cfg(test)]
mod tests;
