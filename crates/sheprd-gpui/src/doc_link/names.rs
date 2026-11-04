//! Stable display and private agent names for document links.

use std::path::Path;

/// The document tab and pane label, kept safe for a CLI option.
pub(crate) fn tab_label(path: &Path) -> String {
    // i18n: data
    let stem = path
        .file_stem()
        .filter(|stem| *stem != std::ffi::OsStr::new(".md"))
        .map(|stem| stem.to_string_lossy())
        .unwrap_or_default();
    let mut label = sheprd_client::text::safe_text(&stem, 80);
    if label.is_empty() {
        label = "todo".into();
    }
    if label.starts_with('-') {
        label.insert_str(0, "Todo ");
    }
    label
}

/// Produce a short private name whose suffix keeps generated names distinct.
pub(crate) fn agent_name(path: &Path, suffix: u32) -> String {
    let stem = path
        .file_stem()
        .filter(|stem| *stem != std::ffi::OsStr::new(".md"))
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_default();
    let mut slug = String::new();
    for character in stem.to_lowercase().chars() {
        if character.is_ascii_alphanumeric() {
            slug.push(character);
        } else if !slug.ends_with('-') {
            slug.push('-');
        }
    }
    let slug: String = slug
        .trim_matches('-')
        .chars()
        .take(20)
        .collect::<String>()
        .trim_end_matches('-')
        .to_owned();
    let slug = if slug.is_empty() { "todo" } else { &slug };
    format!("doc-{slug}-{suffix:06x}", suffix = suffix & 0x00ff_ffff)
}

pub(crate) fn valid_agent_name(name: &str) -> bool {
    let mut chars = name.chars();
    matches!(chars.next(), Some(first) if first.is_ascii_lowercase())
        && name.len() <= 32
        && chars.all(|character| {
            character.is_ascii_lowercase()
                || character.is_ascii_digit()
                || matches!(character, '_' | '-')
        })
}
