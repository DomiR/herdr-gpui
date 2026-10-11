//! Linear as a source: the open issues assigned to whoever owns the API key,
//! read with Linear's GraphQL API.
//!
//! Unlike the other sources, Linear's listing never reaches the shared
//! database. agent-launcher reads every row of its `issues` table and fails
//! on a provider it does not know, so one Linear row would break its inbox for
//! the whole repository; it also prunes sources it does not sync. The worker
//! keeps the listing in memory instead. Runs on Linear issues are stored like
//! any other, keyed by [`ItemKey::canonical`], which agent-launcher treats as
//! opaque text.

mod http;
mod key;

pub(crate) use http::post;
#[cfg(test)]
pub(crate) use http::{LIMIT, interpret};
#[cfg(test)]
pub(crate) use key::resolve;
pub(crate) use key::{VARIABLE, read as read_key, save as save_key, valid};

use super::{Error, Item, ItemKey, Provider, Result, SourceKey};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::{Value, json};

/// The host every Linear source names; its repository is the workspace.
const HOST: &str = "linear.app";
/// The most issues one sync reads. More are shown as the first ones.
pub(crate) const MAX: usize = 250;

/// Nested `first` values stay small: Linear limits a query's complexity, and
/// nested connections otherwise default to 50.
pub(crate) const QUERY: &str = r#"query($after: String) {
  organization { urlKey }
  viewer {
    assignedIssues(first: 50, after: $after, orderBy: updatedAt,
      filter: { state: { type: { nin: ["completed", "canceled"] } } }) {
      pageInfo { hasNextPage endCursor }
      nodes {
        identifier title description url priority branchName createdAt updatedAt
        state { name }
        creator { displayName }
        parent { identifier }
        labels(first: 10) { nodes { name } }
        inverseRelations(first: 10) { nodes { type issue { identifier } } }
      }
    }
  }
}"#;

/// The source of workspace `workspace`, Linear's `urlKey`.
pub(crate) fn source(workspace: &str) -> SourceKey {
    SourceKey {
        provider: Provider::Linear,
        host: HOST.into(),
        repository: workspace.into(),
    }
}

/// One sync's result. A listing is never written, so `complete` only says
/// whether more issues were left unread.
pub(crate) struct Listing {
    /// The workspace's `urlKey`.
    pub(crate) workspace: String,
    pub(crate) items: Vec<Item>,
    pub(crate) complete: bool,
}

/// Lists the open issues assigned to the key's owner. `fetch` sends the
/// query with its variables and returns the response.
pub(crate) fn sync(
    mut fetch: impl FnMut(Value) -> Result<Value>,
    cancelled: impl Fn() -> bool,
) -> Result<Listing> {
    let mut items = Vec::new();
    let mut workspace: Option<String> = None;
    let mut after = Value::Null;
    let complete = loop {
        if cancelled() {
            return Err(Error::Cancelled);
        }
        let page = page(fetch(json!({ "after": after }))?)?;
        let workspace = workspace.get_or_insert(page.organization.url_key);
        let source = source(workspace);
        let issues = page.viewer.assigned_issues;
        items.extend(issues.nodes.into_iter().map(|node| node.into_item(&source)));
        if items.len() >= MAX {
            let more = items.len() > MAX || issues.page_info.has_next_page;
            items.truncate(MAX);
            break !more;
        }
        match issues.page_info {
            PageInfo {
                has_next_page: true,
                end_cursor: Some(cursor),
            } => after = Value::String(cursor),
            _ => break true,
        }
    };
    Ok(Listing {
        workspace: workspace.unwrap_or_default(),
        items,
        complete,
    })
}

fn page(response: Value) -> Result<Data> {
    let data = response.get("data").cloned().ok_or(Error::Output {
        operation: "Reading Linear",
    })?;
    serde_json::from_value(data).map_err(|source| Error::OutputJson {
        operation: "Reading Linear",
        source,
    })
}

#[derive(Deserialize)]
struct Data {
    organization: Organization,
    viewer: Viewer,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Organization {
    url_key: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Viewer {
    assigned_issues: Connection<Node>,
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
#[serde(rename_all = "camelCase")]
struct Person {
    display_name: String,
}

#[derive(Deserialize)]
struct Identified {
    identifier: String,
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
    identifier: String,
    title: String,
    description: Option<String>,
    url: String,
    priority: Option<f64>,
    branch_name: Option<String>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    state: Named,
    creator: Option<Person>,
    parent: Option<Identified>,
    labels: Option<Connection<Named>>,
    inverse_relations: Option<Connection<Relation>>,
}

impl Node {
    fn into_item(self, source: &SourceKey) -> Item {
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
            parent_id: self.parent.map(|parent| parent.identifier),
            blocked_by,
            priority: self.priority.and_then(priority),
            created_at: Some(self.created_at),
            updated_at: Some(self.updated_at),
            pull_request: None,
            activity: None,
            branch: self.branch_name.filter(|branch| !branch.is_empty()),
        }
    }
}

/// Linear's priority: 1 urgent to 4 low; 0 is none.
fn priority(value: f64) -> Option<i64> {
    let value = value.round();
    (1.0..=4.0).contains(&value).then_some(value as i64)
}
