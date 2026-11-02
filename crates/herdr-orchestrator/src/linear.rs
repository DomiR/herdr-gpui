//! Linear as a source: the open issues assigned to whoever owns the API key,
//! and the open issues in the active cycles of that person's teams, read with
//! Linear's GraphQL API.
//!
//! Unlike the other sources, Linear's listing never reaches the shared
//! database. agent-launcher reads every row of its `issues` table and fails
//! on a provider it does not know, so one Linear row would break its inbox for
//! the whole repository; it also prunes sources it does not sync. The worker
//! keeps the listing in memory instead. Runs on Linear issues are stored like
//! any other, keyed by [`ItemKey::canonical`], which agent-launcher treats as
//! opaque text.

mod app;
mod create;
mod http;
mod key;

pub use app::app_url;
#[cfg(test)]
pub use create::{CREATE, OPTIONS, scale};
pub use create::{
    Choice, Created, CycleChoice, Estimate, Label, NewIssue, Options, TeamOptions, create_issue,
    options,
};
pub use http::post;
#[cfg(test)]
pub use http::{LIMIT, interpret, retry_after};
#[cfg(test)]
pub use key::resolve;
pub use key::{VARIABLE, read as read_key, save as save_key, valid};

use super::{
    Cycle, Error, Item, ItemKey, LinkState, LinkedPullRequest, Progress, Project, Provider, Result,
    SortOrder, SourceKey, Status, StatusKind, SubItem, Tracker,
};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

/// The host every Linear source names; its repository is the workspace.
const HOST: &str = "linear.app";
/// The most issues one sync reads for each scope. More are left unread.
pub const CAP: usize = 500;

/// Nested `first` values stay small: Linear limits a query's complexity to
/// 10,000 points, a connection multiplies its children's by `first`, and
/// nested connections otherwise default to 50. A page of 50 issues, with
/// up to ten sub-issues each, costs a little over half the limit; [`CAP`] is ten pages.
pub const QUERY: &str = r#"query($filter: IssueFilter!, $after: String) {
  organization { urlKey }
  issues(first: 50, after: $after, orderBy: updatedAt, filter: $filter) {
    pageInfo { hasNextPage endCursor }
    nodes {
      id identifier title description url priority branchName createdAt updatedAt sortOrder
      estimate
      state { id name type position color }
      team { id }
      creator { displayName }
      assignee { displayName isMe avatarUrl }
      cycle { number name isActive }
      parent { identifier title }
      project { name color }
      labels(first: 10) { nodes { name } }
      inverseRelations(first: 10) { nodes { type issue { identifier } } }
      attachments(first: 10) { nodes { url metadata } }
      children(first: 10) {
        pageInfo { hasNextPage }
        nodes { identifier title url state { id name type position color } assignee { displayName } }
      }
    }
  }
}"#;

/// Every team's workflow states, which a card can move between. The worker
/// keeps them until the key changes, Refresh, a refused move, or an issue
/// of a team they lack. A team rarely has more than a dozen states; a
/// workspace with more than 50 teams leaves the rest unread, and their
/// issues cannot move.
pub const STATES: &str = r#"query {
  teams(first: 50) {
    nodes { id states(first: 50) { nodes { id name type position color } } }
  }
}"#;

/// Moves one issue to a workflow state, a place among its state's issues,
/// or both, answering with where it is now.
pub const MOVE: &str = r#"mutation($id: String!, $input: IssueUpdateInput!) {
  issueUpdate(id: $id, input: $input) {
    success
    issue { sortOrder state { id name type position color } }
  }
}"#;

/// What one sync lists, one query each. Two queries rather than one `or`
/// filter, which Linear answered with rows matching neither side.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scope {
    /// Open issues assigned to the key's owner.
    Mine,
    /// Open issues in the active cycle of any team the owner belongs to.
    Cycles,
}

impl Scope {
    pub const ALL: [Self; 2] = [Self::Mine, Self::Cycles];

    /// The scope's `IssueFilter`. Both list open issues only, as the Issues
    /// tab does.
    pub fn filter(self) -> Value {
        let open = json!({ "type": { "nin": ["completed", "canceled"] } });
        match self {
            Self::Mine => json!({
                "assignee": { "isMe": { "eq": true } },
                "state": open,
            }),
            Self::Cycles => json!({
                "cycle": { "isActive": { "eq": true } },
                "team": { "members": { "some": { "isMe": { "eq": true } } } },
                "state": open,
            }),
        }
    }
}

/// The source of workspace `workspace`, Linear's `urlKey`.
pub fn source(workspace: &str) -> SourceKey {
    SourceKey {
        provider: Provider::Linear,
        host: HOST.into(),
        repository: workspace.into(),
    }
}

/// One sync's result. A listing is never written, so `complete` only says
/// whether more issues were left unread.
pub struct Listing {
    /// The workspace's `urlKey`.
    pub workspace: String,
    pub items: Vec<Item>,
    pub complete: bool,
    /// An issue's team is not among the states given, so they are read again.
    pub missing_states: bool,
}

/// Each team's workflow states, by team id, in board order.
pub type States = HashMap<String, Arc<Vec<Status>>>;

/// Reads every team's workflow states with [`STATES`]. `fetch` sends a
/// query, as for [`sync`].
pub fn states(mut fetch: impl FnMut(&'static str, Value) -> Result<Value>) -> Result<States> {
    fetch(STATES, json!({})).and_then(teams)
}

/// Lists every [`Scope`], an issue in both once, each with its team's
/// workflow `states`. `fetch` sends a query with its variables and returns
/// the response.
pub fn sync(
    mut fetch: impl FnMut(&'static str, Value) -> Result<Value>,
    states: &States,
    cancelled: impl Fn() -> bool,
) -> Result<Listing> {
    if cancelled() {
        return Err(Error::Cancelled);
    }
    let mut missing_states = false;
    let mut items = Vec::new();
    let mut seen = HashSet::new();
    let mut workspace: Option<String> = None;
    let mut complete = true;
    for scope in Scope::ALL {
        let mut read = 0;
        let mut after = Value::Null;
        loop {
            if cancelled() {
                return Err(Error::Cancelled);
            }
            let page = page(fetch(
                QUERY,
                json!({ "filter": scope.filter(), "after": after }),
            )?)?;
            let workspace = workspace.get_or_insert(page.organization.url_key);
            let source = source(workspace);
            let Connection { page_info, nodes } = page.issues;
            let room = CAP - read;
            let more = nodes.len() > room || page_info.has_next_page;
            for node in nodes.into_iter().take(room) {
                read += 1;
                if seen.insert(node.identifier.clone()) {
                    missing_states |= node
                        .team
                        .as_ref()
                        .is_some_and(|team| !states.contains_key(&team.id));
                    items.push(node.into_item(&source, states));
                }
            }
            if read >= CAP {
                complete &= !more;
                break;
            }
            match page_info {
                PageInfo {
                    has_next_page: true,
                    end_cursor: Some(cursor),
                } => after = Value::String(cursor),
                _ => break,
            }
        }
    }
    Ok(Listing {
        workspace: workspace.unwrap_or_default(),
        items,
        complete,
        missing_states,
    })
}

fn page(response: Value) -> Result<Data> {
    data(response, "Reading Linear")
}

/// `response`'s `data`, read as `T`.
fn data<T: serde::de::DeserializeOwned>(response: Value, operation: &'static str) -> Result<T> {
    let data = response
        .get("data")
        .cloned()
        .ok_or(Error::Output { operation })?;
    serde_json::from_value(data).map_err(|source| Error::OutputJson { operation, source })
}

fn teams(response: Value) -> Result<States> {
    let teams: Teams = data(response, "Reading Linear's workflow states")?;
    Ok(teams
        .teams
        .nodes
        .into_iter()
        .map(|team| {
            let mut states: Vec<Status> = team.states.nodes.iter().map(StateNode::status).collect();
            states.sort_by(|a, b| a.order().cmp(&b.order()));
            (team.id, Arc::new(states))
        })
        .collect())
}

/// A card dropped in another lane or another place in its own: issue
/// `issue` to the state `state` names, to the place `sort_order` says, or
/// both. The view sends it; the worker, which alone holds the key, asks
/// Linear.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Move {
    /// The issue's UUID, else its identifier, which Linear also accepts.
    pub issue: String,
    /// What the user reads, such as `ENG-12`, for a failure.
    pub identifier: String,
    /// The workflow state's id, when the issue changes state.
    pub state: Option<String>,
    /// Its `sortOrder` among the state's issues, when it has neighbours to
    /// sit between.
    pub sort_order: Option<SortOrder>,
    /// The view's count of moves, which the worker's snapshot echoes once
    /// this one is done, so the view stops showing it ahead of Linear.
    pub sequence: u64,
}

impl Move {
    /// The `IssueUpdateInput` asking for this move: only what changes.
    pub fn input(&self) -> Value {
        let mut input = serde_json::Map::new();
        if let Some(state) = &self.state {
            input.insert("stateId".into(), json!(state));
        }
        if let Some(SortOrder(order)) = self.sort_order {
            input.insert("sortOrder".into(), json!(order));
        }
        Value::Object(input)
    }
}

/// Where an issue is after a move, as Linear answered.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Placed {
    pub status: Status,
    pub sort_order: Option<SortOrder>,
}

/// Moves `change`'s issue, returning where Linear says it is now. `fetch`
/// sends a query with its variables, as for [`sync`].
pub fn move_issue(
    mut fetch: impl FnMut(&'static str, Value) -> Result<Value>,
    change: &Move,
) -> Result<Placed> {
    const OPERATION: &str = "Moving a Linear issue";
    let answer: Moved = data(
        fetch(MOVE, json!({ "id": change.issue, "input": change.input() }))?,
        OPERATION,
    )?;
    match answer.issue_update {
        Updated {
            success: true,
            issue: Some(issue),
        } => Ok(Placed {
            status: issue.state.status(),
            sort_order: issue.sort_order.map(SortOrder),
        }),
        _ => Err(Error::Output {
            operation: OPERATION,
        }),
    }
}

#[derive(Deserialize)]
struct Teams {
    teams: Connection<Team>,
}

#[derive(Deserialize)]
struct Team {
    id: String,
    states: Connection<StateNode>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Moved {
    issue_update: Updated,
}

#[derive(Deserialize)]
struct Updated {
    success: bool,
    issue: Option<MovedIssue>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct MovedIssue {
    sort_order: Option<f64>,
    state: StateNode,
}

#[derive(Deserialize)]
struct Data {
    organization: Organization,
    issues: Connection<Node>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Organization {
    url_key: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Connection<T> {
    #[serde(default)]
    page_info: PageInfo,
    nodes: Vec<T>,
}

#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PageInfo {
    has_next_page: bool,
    end_cursor: Option<String>,
}

#[derive(Deserialize)]
struct Named {
    name: String,
}

#[derive(Deserialize)]
struct StateNode {
    id: Option<String>,
    name: String,
    #[serde(rename = "type", default)]
    kind: String,
    #[serde(default)]
    position: f64,
    color: Option<String>,
}

impl StateNode {
    fn status(&self) -> Status {
        Status {
            name: self.name.clone(),
            kind: StatusKind::parse(&self.kind),
            rank: (self.position * 1000.).round() as i64,
            color: self.color.as_deref().and_then(hex_color),
            id: self.id.clone(),
        }
    }
}

/// `#RRGGBB` as `0xRRGGBB`.
fn hex_color(color: &str) -> Option<u32> {
    let digits = color.strip_prefix('#')?;
    (digits.len() == 6)
        .then(|| u32::from_str_radix(digits, 16).ok())
        .flatten()
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Person {
    display_name: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Assignee {
    display_name: String,
    is_me: bool,
    avatar_url: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CycleNode {
    /// A whole number, though Linear's schema calls it a Float.
    number: f64,
    name: Option<String>,
    is_active: bool,
}

#[derive(Deserialize)]
struct Identified {
    identifier: String,
}

#[derive(Deserialize)]
struct Parent {
    identifier: String,
    title: Option<String>,
}

#[derive(Deserialize)]
struct ProjectNode {
    name: String,
    color: Option<String>,
}

/// A link on an issue, such as a GitHub pull request Linear's integration
/// attached, with what the integration says about it.
#[derive(Deserialize)]
struct Attachment {
    url: String,
    #[serde(default)]
    metadata: Value,
}

impl Attachment {
    /// The pull request the link names, when it is a GitHub one.
    fn pull_request(&self) -> Option<LinkedPullRequest> {
        let url = url::Url::parse(&self.url).ok()?;
        if url.scheme() != "https" || url.host_str() != Some("github.com") {
            return None;
        }
        let mut segments = url.path_segments()?;
        let (Some(_owner), Some(_repo), Some("pull"), Some(number)) = (
            segments.next(),
            segments.next(),
            segments.next(),
            segments.next(),
        ) else {
            return None;
        };
        let draft = self.metadata.get("draft").and_then(Value::as_bool) == Some(true);
        let state = match self.metadata.get("status").and_then(Value::as_str) {
            Some("merged") => Some(LinkState::Merged),
            Some("closed") => Some(LinkState::Closed),
            Some("draft") => Some(LinkState::Draft),
            // Open, or a review state of an open one, such as `inReview`.
            Some(_) if draft => Some(LinkState::Draft),
            Some(_) => Some(LinkState::Open),
            None => None,
        };
        Some(LinkedPullRequest {
            url: self.url.clone(),
            number: number.parse().ok()?,
            state,
        })
    }
}

/// A sub-issue, read only for its state's kind.
#[derive(Deserialize)]
struct Child {
    #[serde(default)]
    identifier: String,
    #[serde(default)]
    title: String,
    url: Option<String>,
    state: StateNode,
    assignee: Option<Person>,
}

impl Child {
    fn into_sub_item(self) -> SubItem {
        SubItem {
            status: Some(self.state.status()),
            identifier: self.identifier,
            title: self.title,
            url: self.url.filter(|url| !url.is_empty()),
            assignee: self.assignee.map(|person| person.display_name),
        }
    }
}

/// How many of `children` are done.
fn progress(children: &Connection<Child>) -> Option<Progress> {
    let kinds: Vec<StatusKind> = children
        .nodes
        .iter()
        .map(|child| StatusKind::parse(&child.state.kind))
        .filter(|kind| *kind != StatusKind::Canceled)
        .collect();
    let done = kinds
        .iter()
        .filter(|kind| **kind == StatusKind::Completed)
        .count();
    let more = children.page_info.has_next_page;
    (!kinds.is_empty() || more).then(|| Progress {
        done: u32::try_from(done).unwrap_or(u32::MAX),
        total: u32::try_from(kinds.len()).unwrap_or(u32::MAX),
        more,
    })
}

#[derive(Deserialize)]
struct Relation {
    #[serde(rename = "type")]
    kind: String,
    issue: Identified,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Node {
    id: Option<String>,
    identifier: String,
    title: String,
    description: Option<String>,
    url: String,
    priority: Option<f64>,
    branch_name: Option<String>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    sort_order: Option<f64>,
    estimate: Option<f64>,
    state: StateNode,
    team: Option<TeamRef>,
    creator: Option<Person>,
    assignee: Option<Assignee>,
    cycle: Option<CycleNode>,
    parent: Option<Parent>,
    project: Option<ProjectNode>,
    labels: Option<Connection<Named>>,
    inverse_relations: Option<Connection<Relation>>,
    attachments: Option<Connection<Attachment>>,
    children: Option<Connection<Child>>,
}

#[derive(Deserialize)]
struct TeamRef {
    id: String,
}

impl Node {
    fn into_item(self, source: &SourceKey, states: &States) -> Item {
        // A relation `issue blocks relatedIssue` is listed on the blocked
        // issue as an inverse relation, so its `issue` is the blocker.
        let mut blocked_by: Vec<String> = self
            .inverse_relations
            .map(|relations| relations.nodes)
            .unwrap_or_default()
            .into_iter()
            .filter(|relation| relation.kind == "blocks")
            .map(|relation| relation.issue.identifier)
            .collect();
        blocked_by.sort_unstable();
        blocked_by.dedup();
        let status = self.state.status();
        let mut pull_requests: Vec<LinkedPullRequest> = Vec::new();
        for link in self
            .attachments
            .iter()
            .flat_map(|attachments| &attachments.nodes)
            .filter_map(Attachment::pull_request)
        {
            if !pull_requests.iter().any(|known| known.url == link.url) {
                pull_requests.push(link);
            }
        }
        let (parent_id, parent) = self
            .parent
            .map(|parent| (parent.identifier, parent.title))
            .unzip();
        Item {
            key: ItemKey {
                source: source.clone(),
                native_id: self.identifier.clone(),
            },
            identifier: self.identifier,
            title: self.title,
            description: self.description.filter(|text| !text.is_empty()),
            state: self.state.name.to_lowercase(),
            url: Some(self.url),
            author: self.creator.map(|creator| creator.display_name),
            labels: self
                .labels
                .map(|labels| labels.nodes.into_iter().map(|label| label.name).collect())
                .unwrap_or_default(),
            parent_id,
            blocked_by,
            priority: self.priority.and_then(priority),
            created_at: Some(self.created_at),
            updated_at: Some(self.updated_at),
            pull_request: None,
            activity: None,
            tracker: Some(Tracker {
                branch: self.branch_name.filter(|branch| !branch.is_empty()),
                mine: self
                    .assignee
                    .as_ref()
                    .is_some_and(|assignee| assignee.is_me),
                avatar: self
                    .assignee
                    .as_ref()
                    .and_then(|assignee| assignee.avatar_url.clone())
                    .filter(|url| !url.is_empty()),
                assignee: self.assignee.map(|assignee| assignee.display_name),
                cycle: self.cycle.map(|cycle| Cycle {
                    number: cycle.number.max(0.).round() as u64,
                    name: cycle.name.filter(|name| !name.is_empty()),
                    active: cycle.is_active,
                }),
                status: Some(status),
                id: self.id,
                states: self
                    .team
                    .and_then(|team| states.get(&team.id).cloned())
                    .unwrap_or_default(),
                sort_order: self.sort_order.map(SortOrder),
                estimate: self
                    .estimate
                    .filter(|estimate| estimate.is_finite() && *estimate >= 0.)
                    .map(|estimate| estimate.round() as u32),
                project: self.project.map(|project| Project {
                    color: project.color.as_deref().and_then(hex_color),
                    name: project.name,
                }),
                parent: parent.flatten().filter(|title| !title.is_empty()),
                pull_requests,
                progress: self.children.as_ref().and_then(progress),
                children: self
                    .children
                    .map(|children| children.nodes)
                    .unwrap_or_default()
                    .into_iter()
                    .filter(|child| !child.identifier.is_empty())
                    .map(Child::into_sub_item)
                    .collect(),
            }),
        }
    }
}

/// Linear's priority: 1 urgent to 4 low; 0 is none.
fn priority(value: f64) -> Option<i64> {
    let value = value.round();
    (1.0..=4.0).contains(&value).then_some(value as i64)
}
