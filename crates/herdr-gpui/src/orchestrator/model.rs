//! Records in the shapes the shared database stores them. JSON columns use
//! agent-launcher's serde field names, so either application decodes the
//! other's rows.

use super::{Error, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, path::PathBuf, str::FromStr};

/// Where an item comes from. Linear items are never in the shared
/// database: agent-launcher cannot read a `linear` row.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub(crate) enum Provider {
    Github,
    Gitlab,
    Beads,
    Linear,
}

impl Provider {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Github => "github",
            Self::Gitlab => "gitlab",
            Self::Beads => "beads",
            Self::Linear => "linear",
        }
    }

    /// What the user reads, such as "GitHub".
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Github => "GitHub",
            Self::Gitlab => "GitLab",
            Self::Beads => "Beads",
            Self::Linear => "Linear",
        }
    }
}

impl FromStr for Provider {
    type Err = Error;

    fn from_str(value: &str) -> Result<Self> {
        match value {
            "github" => Ok(Self::Github),
            "gitlab" => Ok(Self::Gitlab),
            "beads" => Ok(Self::Beads),
            "linear" => Ok(Self::Linear),
            other => Err(Error::Provider(other.to_owned())),
        }
    }
}

/// Escapes one key component so joined keys stay injective.
fn component(value: &str) -> String {
    value.replace('%', "%25").replace(':', "%3A")
}

/// Reverses [`component`]: `%3A` first, so an escaped `%253A` stays `%3A`.
fn uncomponent(value: &str) -> String {
    value.replace("%3A", ":").replace("%25", "%")
}

/// One synced source: a provider's view of one repository.
#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub(crate) struct SourceKey {
    pub(crate) provider: Provider,
    pub(crate) host: String,
    pub(crate) repository: String,
}

impl SourceKey {
    /// `provider:host:repository`, the `source` column of issues and
    /// checkpoints.
    pub(crate) fn canonical(&self) -> String {
        format!(
            "{}:{}:{}",
            self.provider.as_str(),
            component(&self.host),
            component(&self.repository)
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct ItemKey {
    pub(crate) source: SourceKey,
    /// A GitHub issue's number, `pr/<number>` for a pull request, a Beads id,
    /// or a Linear identifier such as `ENG-12`.
    pub(crate) native_id: String,
}

impl ItemKey {
    pub(crate) fn canonical(&self) -> String {
        format!("{}:{}", self.source.canonical(), component(&self.native_id))
    }
}

/// Parses [`ItemKey::canonical`]: escaped components never hold a `:`.
impl FromStr for ItemKey {
    type Err = Error;

    fn from_str(value: &str) -> Result<Self> {
        let parts: Vec<&str> = value.split(':').collect();
        let [provider, host, repository, native_id] = parts[..] else {
            return Err(Error::ItemKey(value.to_owned()));
        };
        if native_id.is_empty() {
            return Err(Error::ItemKey(value.to_owned()));
        }
        Ok(Self {
            source: SourceKey {
                provider: provider.parse()?,
                host: uncomponent(host),
                repository: uncomponent(repository),
            },
            native_id: uncomponent(native_id),
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct PullRequest {
    pub(crate) number: u64,
    pub(crate) additions: Option<u64>,
    pub(crate) deletions: Option<u64>,
    pub(crate) base_ref: String,
    pub(crate) head_ref: String,
    pub(crate) base_sha: String,
    pub(crate) head_sha: String,
    /// The repository holding the head branch, as `owner/name`.
    pub(crate) head_repository: Option<String>,
}

impl Item {
    /// What a run's page shows when its item is no longer listed, such as
    /// an issue closed since: the key alone, so the run keeps its controls.
    pub(crate) fn stand_in(key: ItemKey) -> Self {
        let number = key.native_id.strip_prefix("pr/").unwrap_or(&key.native_id);
        let identifier = match key.source.provider {
            Provider::Beads | Provider::Linear => key.native_id.clone(),
            _ => format!("#{number}"),
        };
        Self {
            key,
            identifier,
            title: "No longer listed".into(),
            description: None,
            state: "unlisted".into(),
            url: None,
            author: None,
            labels: Vec::new(),
            parent_id: None,
            blocked_by: Vec::new(),
            priority: None,
            created_at: None,
            updated_at: None,
            pull_request: None,
            activity: None,
            tracker: None,
        }
    }

    /// The priority as the user reads it: `P1`, or Linear's own words.
    pub(crate) fn priority_label(&self) -> Option<String> {
        let priority = self.priority?;
        let word = match (self.key.source.provider, priority) {
            (Provider::Linear, 1) => "Urgent",
            (Provider::Linear, 2) => "High",
            (Provider::Linear, 3) => "Medium",
            (Provider::Linear, 4) => "Low",
            _ => return Some(format!("P{priority}")),
        };
        Some(word.to_owned())
    }
}

/// Provider-reported engagement counts.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Activity {
    pub(crate) comments: Option<u64>,
    pub(crate) review_comments: Option<u64>,
    pub(crate) commits: Option<u64>,
}

/// An issue, pull request, or bead.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Item {
    pub(crate) key: ItemKey,
    /// What the user reads: `#377`, a Beads id, or a Linear identifier.
    pub(crate) identifier: String,
    pub(crate) title: String,
    pub(crate) description: Option<String>,
    /// The provider's own state word, such as `open`, `merged`, or `in_progress`.
    pub(crate) state: String,
    pub(crate) url: Option<String>,
    pub(crate) author: Option<String>,
    pub(crate) labels: Vec<String>,
    pub(crate) parent_id: Option<String>,
    pub(crate) blocked_by: Vec<String>,
    pub(crate) priority: Option<i64>,
    pub(crate) created_at: Option<DateTime<Utc>>,
    pub(crate) updated_at: Option<DateTime<Utc>>,
    pub(crate) pull_request: Option<PullRequest>,
    pub(crate) activity: Option<Activity>,
    /// What a tracker the shared database does not hold, such as Linear,
    /// says about the item. Never stored, so rows read from it have none.
    pub(crate) tracker: Option<Tracker>,
}

/// An item's details from a tracker the shared database does not hold.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Tracker {
    /// The branch the tracker names for work on the item, such as Linear's
    /// `branchName`.
    pub(crate) branch: Option<String>,
    /// The assignee's display name.
    pub(crate) assignee: Option<String>,
    /// Assigned to the account the tracker is read as.
    pub(crate) mine: bool,
    pub(crate) cycle: Option<Cycle>,
}

/// A Linear cycle, the team's sprint.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Cycle {
    pub(crate) number: u64,
    /// Most cycles have none.
    pub(crate) name: Option<String>,
    pub(crate) active: bool,
}

impl Cycle {
    /// What the user reads: the cycle's name, else `Cycle <number>`.
    pub(crate) fn label(&self) -> String {
        match &self.name {
            Some(name) => name.clone(),
            None => format!("Cycle {}", self.number),
        }
    }
}

/// A source's sync position. `pr_details` holds agent-launcher's opaque PR
/// revision markers; herdr-gpui keeps the ones it finds and adds none.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Checkpoint {
    pub(crate) updated_at: Option<DateTime<Utc>>,
    pub(crate) etag: Option<String>,
    pub(crate) last_full_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub(crate) pr_details: HashMap<String, String>,
    #[serde(default)]
    pub(crate) pr_cursor: Option<u64>,
}

/// Which application dispatched a run, and so alone may write it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Owner {
    AgentLauncher,
    HerdrGpui,
    /// No application: a run found from its item's branch, never stored.
    Branch,
}

impl Owner {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::AgentLauncher => "agent-launcher",
            Self::HerdrGpui => "herdr-gpui",
            Self::Branch => "branch",
        }
    }
}

impl FromStr for Owner {
    type Err = Error;

    fn from_str(value: &str) -> Result<Self> {
        match value {
            "agent-launcher" => Ok(Self::AgentLauncher),
            "herdr-gpui" => Ok(Self::HerdrGpui),
            other => Err(Error::Owner(other.to_owned())),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum RunState {
    Provisioning,
    Starting,
    Running,
    NeedsInput,
    Idle,
    Completed,
    Failed,
    Cancelled,
    Disconnected,
}

/// The agent-launcher backend a run went through; herdr-gpui only uses Herdr.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Backend {
    Superset,
    Native,
    Herdr,
    Conductor,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Workspace {
    pub(crate) backend: Backend,
    pub(crate) id: String,
    pub(crate) host: Option<String>,
    pub(crate) path: Option<PathBuf>,
    pub(crate) branch: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Run {
    pub(crate) id: String,
    /// The canonical key of the item the run works on.
    pub(crate) item_key: String,
    pub(crate) workspace: Option<Workspace>,
    pub(crate) agent: String,
    pub(crate) model: Option<String>,
    pub(crate) state: RunState,
    pub(crate) message: Option<String>,
    pub(crate) session_id: Option<String>,
    pub(crate) started_at: DateTime<Utc>,
    pub(crate) updated_at: DateTime<Utc>,
    pub(crate) owner: Owner,
}

impl Run {
    /// When the run began, unknown for one found from its branch.
    pub(crate) fn started(&self) -> Option<DateTime<Utc>> {
        (self.owner != Owner::Branch).then_some(self.started_at)
    }
}

/// Where a run's agent lives in Herdr, written by the run's owner.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct HerdrSession {
    pub(crate) run_id: String,
    /// `None` on the machine that owns the database, else an SSH destination.
    pub(crate) host: Option<String>,
    /// The Herdr `--session` name, `None` for the default session.
    pub(crate) session: Option<String>,
    pub(crate) workspace_id: String,
    pub(crate) pane_id: String,
    pub(crate) agent_name: String,
    pub(crate) updated_at: DateTime<Utc>,
}
