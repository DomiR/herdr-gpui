//! `confirm_close_tab`: when closing a Herdr tab shows its dialog.

use serde::Deserialize;

/// When closing a Herdr tab asks first. Closing one ends its terminals'
/// processes, so the default asks while an agent in it is mid-task.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(from = "Saved")]
pub enum TabCloseConfirm {
    /// Tabs close at once.
    Never,
    /// Only while an agent in the tab is working or blocked on a prompt.
    #[default]
    Busy,
    /// Every tab close asks.
    Always,
}

impl TabCloseConfirm {
    /// Every choice, in the order the settings window shows them.
    pub const ALL: &[Self] = &[Self::Never, Self::Busy, Self::Always];

    /// The message naming it in Settings.
    pub const fn message(self) -> &'static str {
        match self {
            Self::Never => "settings-tabs-confirm-tab-never",
            Self::Busy => "settings-tabs-confirm-tab-busy",
            Self::Always => "settings-tabs-confirm-tab-always",
        }
    }

    /// How Settings titles it.
    pub fn label(self) -> String {
        crate::i18n::message(self.message(), &[])
    }

    /// The value saving writes. The two choices older builds knew keep their
    /// booleans, so a saved file still loads there.
    pub(crate) fn to_toml(self) -> toml_edit::Value {
        match self {
            Self::Never => false.into(),
            Self::Busy => true.into(),
            Self::Always => "always".into(),
        }
    }
}

/// `confirm_close_tab` as a file may hold it: the boolean older builds
/// wrote, or a choice by name.
#[derive(Deserialize)]
#[serde(untagged)]
enum Saved {
    Flag(bool),
    Named(Named),
}

#[derive(Deserialize)]
#[serde(rename_all = "lowercase")]
enum Named {
    Never,
    Busy,
    Always,
}

impl From<Saved> for TabCloseConfirm {
    fn from(saved: Saved) -> Self {
        match saved {
            Saved::Flag(false) | Saved::Named(Named::Never) => Self::Never,
            Saved::Flag(true) | Saved::Named(Named::Busy) => Self::Busy,
            Saved::Named(Named::Always) => Self::Always,
        }
    }
}
