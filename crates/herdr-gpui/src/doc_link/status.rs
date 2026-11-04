//! The app's link status, mapped to control protocol types at that boundary.

use herdr_doc::Version;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Mode {
    // keep-sorted start
    Off,
    On,
    Paused,
    Restoring,
    Starting,
    // keep-sorted end
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Pending {
    pub(crate) from: Version,
    pub(crate) in_flight: bool,
    pub(crate) to: Version,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LinkStatus {
    pub(crate) acked: Option<Version>,
    pub(crate) agent_name: Option<String>,
    pub(crate) mode: Mode,
    pub(crate) pane_id: Option<String>,
    pub(crate) pending: Option<Pending>,
}
