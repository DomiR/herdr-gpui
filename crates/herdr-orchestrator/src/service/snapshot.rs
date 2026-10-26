//! What the worker publishes: the snapshot the view shows, replaced whole on
//! every change, and how each source and action is doing.

use super::{
    Access, Error, HerdrSession, Item, Run, SourceKey, linear_create, prompt::Profile,
    repo::RepoInfo,
};
use chrono::{DateTime, Utc};
use herdr_remote::teleport::AgentKind;
use std::{collections::HashMap, sync::Arc};

/// A pull request looked up for the open item.
#[derive(Clone, Debug)]
pub struct PullRequestLookup {
    pub number: u64,
    pub result: Arc<herdr_github::Result<Option<herdr_github::pull_request::PullRequest>>>,
}

/// How an action the user asked for ended.
#[derive(Clone, Debug)]
pub struct Notice {
    pub outcome: Result<&'static str, Arc<Error>>,
}

/// How one source is doing.
#[derive(Clone, Debug)]
pub enum SyncState {
    Syncing,
    Synced {
        at: DateTime<Utc>,
        count: usize,
    },
    /// Larger than one sync reads; shown, not cached.
    Capped {
        count: usize,
    },
    /// GitHub needs a signed-in account, or Linear an API key.
    SignIn,
    /// A provider or host this build cannot read yet, such as GitHub Enterprise.
    Unsupported,
    Failed(Arc<Error>),
}

#[derive(Clone, Debug)]
pub struct SourceStatus {
    pub key: SourceKey,
    pub state: SyncState,
}

/// Everything the view shows, replaced whole on every change.
#[derive(Clone, Debug, Default)]
pub struct Snapshot {
    pub repo: Option<RepoInfo>,
    pub access: Option<Access>,
    pub items: Arc<Vec<Item>>,
    pub runs: Arc<Vec<Run>>,
    pub sessions: Arc<Vec<HerdrSession>>,
    pub sources: Vec<SourceStatus>,
    /// Opening the repository or its database failed.
    pub error: Option<Arc<Error>>,
    /// agent-launcher's prompt profiles, read once when the view opened.
    pub profiles: Arc<Vec<Profile>>,
    /// The agents installed on the repository's host, once known.
    pub installed: Option<Arc<Vec<AgentKind>>>,
    pub installed_endpoint: Option<String>,
    pub installed_error: Option<Arc<Error>>,
    /// The newest pull request lookup.
    pub pull_request: Option<PullRequestLookup>,
    /// The repository's branches named for items, as each sync read them.
    pub branches: Arc<Vec<String>>,
    /// Each open pull request's review and checks, by repository and number,
    /// as the last sync of its source read them. Never stored.
    pub pr_status: Arc<HashMap<(String, u64), crate::PrStatus>>,
    /// The [`crate::linear::Move::sequence`] of the last move this worker
    /// finished, moved or not: `items` shows its outcome.
    pub linear_moved: u64,
    /// The new issue dialog's choices, and the newest creation's outcome.
    pub linear_create: linear_create::LinearCreate,
}
