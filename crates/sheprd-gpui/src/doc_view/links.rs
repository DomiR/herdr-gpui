//! File references in a Markdown document: inline code, Markdown link targets,
//! and bare prose paths, with `:line`, `:line:column`, and `#Lline` suffixes.
//! Relative paths resolve against the document's folder, then its Git checkout
//! root; `~/` paths resolve against home. All disk checks stay off the UI
//! thread and are remembered per reference, so drawing only reads answers
//! already found.

mod prose;
mod reference;

pub(super) use prose::bare_paths;

use super::editor::DocEditor;
use gpui::{AppContext as _, Context};
use sheprd_doc::markdown::{self, Line, Span, Style};
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
};

/// The most references one check looks up.
const MAX_PROBES: usize = 512;
/// The longest reference taken for a path.
const MAX_PATH: usize = 512;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
/// The source form of a file reference.
pub(super) enum Form {
    Code,
    Prose,
    Target,
}

#[derive(Clone, Copy, Debug)]
/// A borrowed lookup key for the cached reference maps.
pub(super) struct Key<'a> {
    pub(super) form: Form,
    pub(super) text: &'a str,
}

/// A path a reference names, and the line in it, if one follows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct FileLink {
    pub(crate) path: PathBuf,
    pub(crate) line: Option<u32>,
}

#[derive(Clone, Debug, Default)]
/// The document and home roots used by the background probe.
struct Roots {
    dirs: Vec<PathBuf>,
    home: Option<PathBuf>,
}

/// What this view knows of the references it shows.
#[derive(Debug, Default)]
pub(super) struct FileLinks {
    /// Where relative paths resolve, and the home directory, found off the UI thread.
    roots: Option<Roots>,
    known: HashMap<Form, HashMap<String, Option<FileLink>>>,
    /// The latest references waiting for the current background probe.
    pending: Option<Vec<(Form, String)>>,
    probing: bool,
}

impl FileLinks {
    /// The reference `text` names, if it is known to exist.
    pub(super) fn get(&self, form: Form, text: &str) -> Option<&FileLink> {
        self.known.get(&form)?.get(text)?.as_ref()
    }
}

/// Whether a code span's text could be a path.
fn looks_like_path(text: &str) -> bool {
    reference::parse(text, false).is_some()
        && (text.contains('/') || text.contains('.') || text.starts_with("~/"))
}

/// The target text in a Markdown link. Angle-bracket targets retain spaces.
pub(super) fn target<'a>(text: &'a str, span: &Span) -> Option<&'a str> {
    let Style::Link { url } = &span.style else {
        return None;
    };
    let Some(url) = url else {
        return markdown::link_url(text, span);
    };
    let raw = text.get(url.clone())?;
    let trimmed = raw.trim_start();
    if let Some(rest) = trimmed.strip_prefix('<')
        && let Some(end) = rest.find('>')
    {
        return trimmed.get(..end + 2);
    }
    markdown::link_url(text, span)
}

/// Every reference form that can be probed in `lines`.
pub(super) fn candidates(text: &str, lines: &[Line]) -> Vec<(Form, String)> {
    let mut out = Vec::new();
    for line in lines {
        for span in markdown::inline(text, line) {
            match &span.style {
                Style::Code => {
                    let Some(code) = text.get(span.range) else {
                        continue;
                    };
                    if looks_like_path(code) {
                        out.push((Form::Code, code.to_owned()));
                    }
                }
                Style::Link { .. } => {
                    let Some(url) = markdown::link_url(text, &span) else {
                        continue;
                    };
                    let Some(target) = target(text, &span) else {
                        continue;
                    };
                    if super::layout::web(url) || reference::parse(target, true).is_none() {
                        continue;
                    }
                    out.push((Form::Target, target.to_owned()));
                }
                _ => {}
            }
        }
        for range in bare_paths(text, line) {
            if let Some(path) = text.get(range) {
                out.push((Form::Prose, path.to_owned()));
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

/// Looks up one reference. This is called only by the background probe job.
fn probe(roots: &Roots, form: Form, text: &str) -> Option<FileLink> {
    let reference = reference::parse(text, matches!(form, Form::Target))?;
    let path = PathBuf::from(reference.path);
    let candidates: Vec<PathBuf> = if reference.home {
        vec![roots.home.as_ref()?.join(&path)]
    } else if path.is_absolute() {
        vec![path]
    } else {
        roots.dirs.iter().map(|root| root.join(&path)).collect()
    };
    candidates
        .into_iter()
        .find(|candidate| candidate.is_file())
        .map(|path| FileLink {
            path,
            line: reference.line,
        })
}

impl DocEditor {
    /// Starts looking up references no check has answered yet.
    pub(super) fn probe_links(&mut self, wanted: Vec<(Form, String)>, cx: &mut Context<Self>) {
        if self.links.probing {
            self.links.pending = Some(wanted);
            return;
        }
        let wanted: HashSet<(Form, String)> = wanted.into_iter().collect();
        let known_len: usize = self.links.known.values().map(HashMap::len).sum();
        if known_len > 4096 {
            for (form, known) in &mut self.links.known {
                known.retain(|text, _| wanted.contains(&(*form, text.clone())));
            }
        }
        let mut unknown: Vec<(Form, String)> = wanted
            .iter()
            .filter(|(form, text)| {
                !self
                    .links
                    .known
                    .get(form)
                    .is_some_and(|known| known.contains_key(text))
            })
            .cloned()
            .collect();
        let truncated = unknown.len() > MAX_PROBES;
        unknown.truncate(MAX_PROBES);
        if truncated {
            self.links.pending = Some(wanted.into_iter().collect());
        }
        if unknown.is_empty() && self.links.roots.is_some() {
            return;
        }
        self.links.probing = true;
        let document = self.buffer.read(cx).path().to_owned();
        let known_roots = self.links.roots.clone();
        let job = cx.background_spawn(async move {
            let roots = known_roots.unwrap_or_else(|| Roots {
                dirs: roots(&document),
                home: sheprd_client::local_path::home_dir(),
            });
            let found: Vec<((Form, String), Option<FileLink>)> = unknown
                .into_iter()
                .map(|(form, text)| {
                    let link = probe(&roots, form, &text);
                    ((form, text), link)
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
                for ((form, text), link) in found {
                    this.links.known.entry(form).or_default().insert(text, link);
                }
                if any {
                    cx.notify();
                }
                if let Some(pending) = this.links.pending.take() {
                    this.probe_links(pending, cx);
                }
            });
        })
        .detach();
    }
}

#[cfg(test)]
mod tests;
