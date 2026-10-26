//! Creating a Linear issue: what the new issue dialog offers to pick, read
//! with one query the first time it opens, and the `issueCreate` mutation.
//! Only the worker, which holds the API key, sends either.

use super::{Connection, StateNode, data, hex_color};
use crate::{Cycle, Error, Result, Status, StatusKind};
use serde::Deserialize;
use serde_json::{Value, json};

/// The key owner's teams with everything an issue in each can be given.
/// Nested `first` values stay small, as in [`super::QUERY`]: Linear limits
/// a query's complexity, and nested connections multiply. A team past the
/// first 20, or a choice past its connection's first page, is not offered.
pub const OPTIONS: &str = r#"query {
  viewer {
    id
    teams(first: 20) {
      nodes {
        id key name
        issueEstimationType issueEstimationAllowZero issueEstimationExtended
        defaultIssueState { id }
        activeCycle { id }
        states(first: 30) { nodes { id name type position color } }
        members(first: 50) { nodes { id displayName active } }
        projects(first: 30) { nodes { id name completedAt canceledAt } }
        labels(first: 50) { nodes { id name color isGroup } }
        cycles(first: 6, filter: { isPast: { eq: false } }) { nodes { id number name isActive } }
      }
    }
  }
  issueLabels(first: 50, filter: { team: { null: true } }) { nodes { id name color isGroup } }
}"#;

/// Creates one issue, answering with what names it.
pub const CREATE: &str = r#"mutation($input: IssueCreateInput!) {
  issueCreate(input: $input) { success issue { identifier url } }
}"#;

/// The estimates a team without its settings read offers.
const DEFAULT_SCALE: [i64; 6] = [0, 1, 2, 3, 5, 8];

/// What the new issue dialog offers, as one query read it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Options {
    /// The key owner's user id, whom a new issue is assigned to at first.
    pub viewer: String,
    /// The owner's teams, by name.
    pub teams: Vec<TeamOptions>,
}

/// One team and what its issues can be given.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TeamOptions {
    pub id: String,
    /// Such as `ENG`, as identifiers start.
    pub key: String,
    pub name: String,
    /// Its workflow states, in board order.
    pub states: Vec<Status>,
    /// The state its new issues start in, when the team names one.
    pub default_state: Option<String>,
    /// Active members, by name.
    pub members: Vec<Choice>,
    /// Projects not yet completed or canceled, by name.
    pub projects: Vec<Choice>,
    /// The team's labels, then the workspace's, by name; groups, which no
    /// issue carries, are left out.
    pub labels: Vec<Label>,
    /// The active and upcoming cycles, by number.
    pub cycles: Vec<CycleChoice>,
    pub active_cycle: Option<String>,
    /// The estimates it offers; `None` when the team does not estimate.
    pub estimates: Option<Vec<Estimate>>,
}

impl TeamOptions {
    /// The state a new issue starts in: the team's default, else its first
    /// unstarted state, else its first backlog state, else its first.
    pub fn default_state(&self) -> Option<&Status> {
        let named = self.default_state.as_deref().and_then(|id| self.state(id));
        named
            .or_else(|| self.first_of(StatusKind::Unstarted))
            .or_else(|| self.first_of(StatusKind::Backlog))
            .or_else(|| self.states.first())
    }

    pub fn state(&self, id: &str) -> Option<&Status> {
        self.states
            .iter()
            .find(|state| state.id.as_deref() == Some(id))
    }

    fn first_of(&self, kind: StatusKind) -> Option<&Status> {
        self.states.iter().find(|state| state.kind == kind)
    }
}

/// A member or project: what it is called and its id.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Choice {
    pub id: String,
    pub name: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Label {
    pub id: String,
    pub name: String,
    /// `0xRRGGBB`.
    pub color: Option<u32>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CycleChoice {
    pub id: String,
    pub cycle: Cycle,
}

/// One point of a team's estimate scale: the number Linear stores and what
/// the user reads, such as `M` on a T-shirt scale.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Estimate {
    pub value: i64,
    pub label: String,
}

/// A team's estimate scale from its settings, as Linear's own pickers offer
/// it; `None` when the team does not use estimates.
pub fn scale(kind: Option<&str>, zero: bool, extended: bool) -> Option<Vec<Estimate>> {
    let numbers = |values: &[i64]| -> Vec<Estimate> {
        values
            .iter()
            .map(|value| Estimate {
                value: *value,
                label: value.to_string(),
            })
            .collect()
    };
    let (base, more): (&[i64], &[i64]) = match kind {
        Some("notUsed") => return None,
        Some("exponential") => (&[1, 2, 4, 8, 16], &[32, 64]),
        Some("fibonacci" | "tShirt") => (&[1, 2, 3, 5, 8], &[13, 21]),
        Some("linear") => (&[1, 2, 3, 4, 5], &[6, 7]),
        _ => return Some(numbers(&DEFAULT_SCALE)),
    };
    let values: Vec<i64> = zero
        .then_some(0)
        .into_iter()
        .chain(base.iter().copied())
        .chain(more.iter().copied().filter(|_| extended))
        .collect();
    let mut scale = numbers(&values);
    if kind == Some("tShirt") {
        for estimate in &mut scale {
            estimate.label = match estimate.value {
                1 => "XS",
                2 => "S",
                3 => "M",
                5 => "L",
                8 => "XL",
                13 => "XXL",
                21 => "XXXL",
                _ => continue,
            }
            .into();
        }
    }
    Some(scale)
}

/// Reads what the dialog offers. `fetch` sends a query with its variables
/// and returns the response, as for [`super::sync`].
pub fn options(mut fetch: impl FnMut(&'static str, Value) -> Result<Value>) -> Result<Options> {
    let answer: OptionsData = data(fetch(OPTIONS, json!({}))?, "Reading Linear's teams")?;
    let workspace = answer.issue_labels.nodes;
    let mut teams: Vec<TeamOptions> = answer
        .viewer
        .teams
        .nodes
        .into_iter()
        .map(|team| team.options(&workspace))
        .collect();
    teams.sort_by_cached_key(|team| team.name.to_lowercase());
    Ok(Options {
        viewer: answer.viewer.id,
        teams,
    })
}

/// An issue the dialog asks for. What is left unset Linear decides, as its
/// own dialog does.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NewIssue {
    pub team: String,
    pub title: String,
    /// Markdown; empty sends none.
    pub description: String,
    pub state: Option<String>,
    /// Linear's: 1 urgent to 4 low; 0 is none, which is not sent.
    pub priority: i64,
    pub assignee: Option<String>,
    pub project: Option<String>,
    pub estimate: Option<i64>,
    pub labels: Vec<String>,
    pub cycle: Option<String>,
    /// The view's count of creations, which the worker's answer echoes.
    pub sequence: u64,
}

impl NewIssue {
    /// The `IssueCreateInput` asking for this issue: only what is set.
    pub fn input(&self) -> Value {
        let mut input = serde_json::Map::new();
        input.insert("teamId".into(), json!(self.team));
        input.insert("title".into(), json!(self.title));
        if !self.description.is_empty() {
            input.insert("description".into(), json!(self.description));
        }
        let ids = [
            ("stateId", &self.state),
            ("assigneeId", &self.assignee),
            ("projectId", &self.project),
            ("cycleId", &self.cycle),
        ];
        for (field, id) in ids {
            if let Some(id) = id {
                input.insert(field.into(), json!(id));
            }
        }
        if self.priority != 0 {
            input.insert("priority".into(), json!(self.priority));
        }
        if let Some(estimate) = self.estimate {
            input.insert("estimate".into(), json!(estimate));
        }
        if !self.labels.is_empty() {
            input.insert("labelIds".into(), json!(self.labels));
        }
        Value::Object(input)
    }
}

/// The issue Linear made.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Created {
    /// Such as `ENG-12`.
    pub identifier: String,
    pub url: String,
}

/// Creates `issue`. `fetch` sends a query with its variables, as for
/// [`super::sync`].
pub fn create_issue(
    mut fetch: impl FnMut(&'static str, Value) -> Result<Value>,
    issue: &NewIssue,
) -> Result<Created> {
    const OPERATION: &str = "Creating a Linear issue";
    let answer: CreatedData = data(fetch(CREATE, json!({ "input": issue.input() }))?, OPERATION)?;
    match answer.issue_create {
        Payload {
            success: true,
            issue: Some(issue),
        } => Ok(Created {
            identifier: issue.identifier,
            url: issue.url,
        }),
        _ => Err(Error::Output {
            operation: OPERATION,
        }),
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct OptionsData {
    viewer: Viewer,
    issue_labels: Connection<LabelNode>,
}

#[derive(Deserialize)]
struct Viewer {
    id: String,
    teams: Connection<TeamNode>,
}

#[derive(Deserialize)]
struct Id {
    id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct TeamNode {
    id: String,
    key: String,
    name: String,
    issue_estimation_type: Option<String>,
    #[serde(default)]
    issue_estimation_allow_zero: bool,
    #[serde(default)]
    issue_estimation_extended: bool,
    default_issue_state: Option<Id>,
    active_cycle: Option<Id>,
    states: Connection<StateNode>,
    members: Connection<MemberNode>,
    projects: Connection<ProjectNode>,
    labels: Connection<LabelNode>,
    cycles: Connection<CycleNode>,
}

impl TeamNode {
    fn options(self, workspace: &[LabelNode]) -> TeamOptions {
        let mut states: Vec<Status> = self.states.nodes.iter().map(StateNode::status).collect();
        states.sort_by(|a, b| a.order().cmp(&b.order()));
        let mut members: Vec<Choice> = self
            .members
            .nodes
            .into_iter()
            .filter(|member| member.active)
            .map(|member| Choice {
                id: member.id,
                name: member.display_name,
            })
            .collect();
        members.sort_by_cached_key(|member| member.name.to_lowercase());
        let mut projects: Vec<Choice> = self
            .projects
            .nodes
            .into_iter()
            .filter(|project| project.completed_at.is_none() && project.canceled_at.is_none())
            .map(|project| Choice {
                id: project.id,
                name: project.name,
            })
            .collect();
        projects.sort_by_cached_key(|project| project.name.to_lowercase());
        let mut labels: Vec<Label> = self
            .labels
            .nodes
            .iter()
            .chain(workspace)
            .filter(|label| !label.is_group)
            .map(|label| Label {
                id: label.id.clone(),
                name: label.name.clone(),
                color: label.color.as_deref().and_then(hex_color),
            })
            .collect();
        labels.sort_by_cached_key(|label| label.name.to_lowercase());
        let mut cycles: Vec<CycleChoice> = self
            .cycles
            .nodes
            .into_iter()
            .map(|cycle| CycleChoice {
                id: cycle.id,
                cycle: Cycle {
                    number: cycle.number.max(0.).round() as u64,
                    name: cycle.name.filter(|name| !name.is_empty()),
                    active: cycle.is_active,
                },
            })
            .collect();
        cycles.sort_by_key(|choice| choice.cycle.number);
        TeamOptions {
            id: self.id,
            key: self.key,
            name: self.name,
            states,
            default_state: self.default_issue_state.map(|state| state.id),
            members,
            projects,
            labels,
            cycles,
            active_cycle: self.active_cycle.map(|cycle| cycle.id),
            estimates: scale(
                self.issue_estimation_type.as_deref(),
                self.issue_estimation_allow_zero,
                self.issue_estimation_extended,
            ),
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct MemberNode {
    id: String,
    display_name: String,
    #[serde(default = "active")]
    active: bool,
}

fn active() -> bool {
    true
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProjectNode {
    id: String,
    name: String,
    completed_at: Option<String>,
    canceled_at: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct LabelNode {
    id: String,
    name: String,
    color: Option<String>,
    #[serde(default)]
    is_group: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CycleNode {
    id: String,
    /// A whole number, though Linear's schema calls it a Float.
    number: f64,
    name: Option<String>,
    #[serde(default)]
    is_active: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreatedData {
    issue_create: Payload,
}

#[derive(Deserialize)]
struct Payload {
    success: bool,
    issue: Option<CreatedIssue>,
}

#[derive(Deserialize)]
struct CreatedIssue {
    identifier: String,
    url: String,
}
