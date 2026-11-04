use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LinkMode {
    // keep-sorted start
    Off,
    On,
    Paused,
    Restoring,
    Starting,
    // keep-sorted end
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct LinkPending {
    pub from: u64,
    pub in_flight: bool,
    pub to: u64,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct LinkStatus {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub acked: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
    pub mode: LinkMode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pane: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pending: Option<LinkPending>,
}

#[cfg(test)]
mod tests;
