use serde::{Deserialize, Serialize};

/// The maximum pane identifier a link-making request may carry.
pub const MAX_PANE_ID: usize = 256;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LinkTarget {
    Agent,
    Tab,
    Workspace,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LinkMakeOp {
    pub target: LinkTarget,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pane_id: Option<String>,
}

impl LinkMakeOp {
    pub fn bounded(&self) -> bool {
        self.pane_id.as_deref().is_none_or(|pane| {
            !pane.is_empty() && pane.len() <= MAX_PANE_ID && !pane.chars().any(char::is_control)
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Reply {
    pub url: String,
    pub label: String,
    pub markdown: String,
}

#[cfg(test)]
mod tests;
