//! Markdown documents (`docs/markdown-docs-plan.md`): open one in a tab,
//! read it with its version number, and change it as the caller's own
//! writer. A change names the version it was made against, and optionally
//! the section it works in; the app refuses a stale one with the diff since.

use serde::{Deserialize, Serialize};

/// The longest text one change may carry, in bytes.
pub const MAX_TEXT: usize = 64 * 1024;
/// The longest a change waits for the document to load and be written.
pub const WAIT_SECONDS: u64 = 15;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum DocOp {
    /// Opens the file in a document tab, at `line` (1-based); answered
    /// as `tab code` is, with the tab.
    Open { path: String, line: Option<u32> },
    /// The text, or one section's, and the version.
    Read {
        path: String,
        #[serde(default)]
        section: Option<String>,
    },
    /// What changed since version `since`.
    Diff { path: String, since: u64 },
    /// The sections headings start, by ID.
    Sections { path: String },
    /// The tasks, or one section's.
    TodoList {
        path: String,
        #[serde(default)]
        section: Option<String>,
    },
    /// Adds a block at the end of the document or of `section`.
    Append {
        path: String,
        version: u64,
        #[serde(default)]
        section: Option<String>,
        text: String,
    },
    /// Replaces the one place `old` occurs with `new`.
    Edit {
        path: String,
        version: u64,
        #[serde(default)]
        section: Option<String>,
        old: String,
        new: String,
    },
    /// Adds a task under `section`, else after the last task.
    TodoAdd {
        path: String,
        version: u64,
        #[serde(default)]
        section: Option<String>,
        text: String,
    },
    /// Checks (or unchecks) the task `todo` names: its number or its text.
    TodoCheck {
        path: String,
        version: u64,
        #[serde(default)]
        section: Option<String>,
        todo: String,
        done: bool,
    },
}

impl DocOp {
    pub fn path(&self) -> &str {
        match self {
            Self::Open { path, .. }
            | Self::Read { path, .. }
            | Self::Diff { path, .. }
            | Self::Sections { path }
            | Self::TodoList { path, .. }
            | Self::Append { path, .. }
            | Self::Edit { path, .. }
            | Self::TodoAdd { path, .. }
            | Self::TodoCheck { path, .. } => path,
        }
    }

    /// How long the app may take: loading the file, and writing a change.
    pub fn wait_seconds(&self) -> u64 {
        match self {
            Self::Open { .. } => 0,
            _ => WAIT_SECONDS,
        }
    }

    /// Whether every text it carries is within [`MAX_TEXT`].
    pub fn bounded(&self) -> bool {
        let texts: &[&str] = match self {
            Self::Append { text, .. } | Self::TodoAdd { text, .. } => &[text],
            Self::Edit { old, new, .. } => &[old, new],
            Self::TodoCheck { todo, .. } => &[todo],
            _ => &[],
        };
        texts.iter().all(|text| text.len() <= MAX_TEXT) && self.path().len() <= 4096
    }
}

/// One section, as `doc sections` lists it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SectionInfo {
    pub id: String,
    pub title: String,
    pub level: u8,
    /// 1-based.
    pub line: usize,
}

/// One task, as `doc todo list` lists it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TodoInfo {
    pub number: usize,
    /// 1-based.
    pub line: usize,
    pub done: bool,
    pub text: String,
    pub depth: usize,
    pub section: Option<String>,
}

/// What changed in other sections while a section change still applied.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Others {
    pub since: u64,
    /// A unified diff; `None` when too large.
    pub diff: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Reply {
    Text {
        path: String,
        version: u64,
        section: Option<String>,
        text: String,
    },
    Diff {
        path: String,
        since: u64,
        version: u64,
        /// Empty when nothing changed; `None` when `since` is too old or
        /// the diff too large.
        diff: Option<String>,
    },
    Sections {
        path: String,
        version: u64,
        sections: Vec<SectionInfo>,
    },
    Todos {
        path: String,
        version: u64,
        todos: Vec<TodoInfo>,
    },
    /// The change is in the document and written to the file.
    Updated {
        path: String,
        version: u64,
        section: Option<String>,
        /// Set when the version named was stale but the section was not.
        others: Option<Others>,
    },
    /// The version named was stale: nothing changed.
    Mismatch {
        path: String,
        expected: u64,
        version: u64,
        /// A unified diff from the version named; `None` when too old or
        /// too large.
        diff: Option<String>,
    },
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn ops_have_a_stable_wire_form() {
        let op = DocOp::TodoCheck {
            path: "/a.md".into(),
            version: 4,
            section: Some("goals".into()),
            todo: "2".into(),
            done: true,
        };
        let text = serde_json::to_string(&op).unwrap();
        assert_eq!(
            text,
            r#"{"op":"todo_check","path":"/a.md","version":4,"section":"goals","todo":"2","done":true}"#
        );
        assert_eq!(serde_json::from_str::<DocOp>(&text).unwrap(), op);
        let read: DocOp = serde_json::from_str(r#"{"op":"read","path":"/a.md"}"#).unwrap();
        assert_eq!(
            read,
            DocOp::Read {
                path: "/a.md".into(),
                section: None
            }
        );
    }

    #[test]
    fn a_change_needs_a_version_and_unknown_fields_are_refused() {
        for bad in [
            r#"{"op":"append","path":"/a.md","text":"x"}"#,
            r#"{"op":"todo_add","path":"/a.md","version":1,"text":"x","force":true}"#,
            r#"{"op":"delete","path":"/a.md"}"#,
        ] {
            assert!(serde_json::from_str::<DocOp>(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn texts_are_bounded() {
        let big = "x".repeat(MAX_TEXT + 1);
        let op = DocOp::Append {
            path: "/a.md".into(),
            version: 1,
            section: None,
            text: big,
        };
        assert!(!op.bounded());
        assert!(
            DocOp::Read {
                path: "/a.md".into(),
                section: None
            }
            .bounded()
        );
    }

    #[test]
    fn replies_round_trip() {
        for reply in [
            Reply::Updated {
                path: "/a.md".into(),
                version: 7,
                section: Some("goals".into()),
                others: Some(Others {
                    since: 5,
                    diff: Some("@@".into()),
                }),
            },
            Reply::Mismatch {
                path: "/a.md".into(),
                expected: 5,
                version: 7,
                diff: None,
            },
        ] {
            let text = serde_json::to_string(&reply).unwrap();
            assert_eq!(serde_json::from_str::<Reply>(&text).unwrap(), reply);
        }
    }
}
