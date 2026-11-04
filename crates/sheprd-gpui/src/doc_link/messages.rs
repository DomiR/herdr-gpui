//! English protocol messages sent to the document conductor.
// i18n: data

use super::Range;
use sheprd_doc::Version;
use std::path::Path;

pub(crate) const INLINE_BYTES: usize = 8 * 1024;
pub(crate) const INLINE_LINES: usize = 120;

pub(crate) fn start(path: &Path, version: Version) -> String {
    let path = path.display();
    format!(
        "/herdrdocwatch {path}\n\nHERDR_DOC_PUSH v1\nMode: on\nDocument: {path}\nVersion: {version}\nSheprd pushes this document's settled changes to you as prompts starting with HERDR_DOC_PUSH v1. Do not start doc watch or a debounce script for it, and stop your own watch of it if one runs. Keep the inbox watch."
    )
}

/// Inline diffs fit under both protocol limits and use a delimiter that does
/// not occur in the diff body.
pub(crate) fn change(path: &Path, range: Range, diff: Option<&str>) -> String {
    let file = file_name(path);
    if let Some(diff) = diff.filter(|diff| {
        !diff.is_empty() && diff.len() <= INLINE_BYTES && diff.lines().count() <= INLINE_LINES
    }) {
        let fence = "~".repeat(fence_len(diff));
        let body = diff.strip_suffix('\n').unwrap_or(diff);
        return format!(
            "HERDR_DOC_PUSH v1\nMode: change\nDocument: {}\nVersions: {} to {}\n{file} changed. Settled net diff below; it can include your own status lines, which are not new work.\n{fence}diff\n{body}\n{fence}",
            path.display(),
            range.from,
            range.to
        );
    }
    format!(
        "HERDR_DOC_PUSH v1\nMode: change\nDocument: {}\nVersions: {} to {}\n{file} changed, now version {}. Run doc diff {} --since {}. If that fails, run doc read on it instead.",
        path.display(),
        range.from,
        range.to,
        range.to,
        sheprd_client::shell_quote(&path.display().to_string()),
        range.from
    )
}

pub(crate) fn resync(path: &Path, version: Version) -> String {
    format!(
        "HERDR_DOC_PUSH v1\nMode: resync\nDocument: {}\nVersion: {}\nSheprd restarted, so document versions started again. Read the document once and reconcile your saved state with live agents; do not redispatch tasks already assigned. Pushes stay on: do not start a watch.",
        path.display(),
        version
    )
}

/// `acked` is the last version the agent saw and `version` is the current one.
pub(crate) fn off(path: &Path, acked: Version, version: Version) -> String {
    format!(
        "HERDR_DOC_PUSH v1\nMode: off\nDocument: {}\nVersion: {}\nPushes for this document are off. Changes after version {} were not pushed. To keep following it, start your own doc watch with --since {}.",
        path.display(),
        version,
        acked,
        acked
    )
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn fence_len(diff: &str) -> usize {
    let mut longest = 0usize;
    let mut current = 0;
    for character in diff.chars() {
        if character == '~' {
            current += 1;
            longest = longest.max(current);
        } else {
            current = 0;
        }
    }
    longest.saturating_add(1).max(3)
}
