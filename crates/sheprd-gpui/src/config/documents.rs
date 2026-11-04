//! `[documents]`: the appearance preset for Markdown document tabs.
use serde::Deserialize;

/// Client-local appearance settings for Markdown document tabs.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct DocumentsConfig {
    /// The colors and task markers used by document tabs.
    pub(crate) theme: DocumentTheme,
}

/// A preset for the appearance of Markdown document tabs.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum DocumentTheme {
    /// Use the app theme and the earlier document colors.
    App,
    /// Use the PlainTasks yellow notepad appearance. This is the default.
    #[default]
    PlainTasks,
}

impl DocumentTheme {
    /// The TOML value for this preset.
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::App => "app",
            Self::PlainTasks => "plaintasks",
        }
    }
}
