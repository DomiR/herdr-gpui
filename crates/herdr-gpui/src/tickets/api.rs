//! herdr-dog's ticket API as the native board reads it: the wire types of
//! `packages/shared/src/tickets.ts` and `comments.ts`, and one blocking call
//! per request. Only background tasks call these. Requests carry no `Origin`
//! header, which herdr-dog accepts from local clients, and mutations send
//! JSON as it requires.

use super::{Error, Result};
use crate::browser::WebUrl;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::{io::Read, time::Duration};

const TIMEOUT: Duration = Duration::from_secs(10);
/// The most of an answer read: a project's board with every description
/// is a few hundred kilobytes for a hundred tickets.
const LIMIT: u64 = 16 * 1024 * 1024;
/// Characters of herdr-dog's own error message kept for display.
const MESSAGE: usize = 240;

/// The board lanes, in lane order; anything newer is kept off the board.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum Status {
    Backlog,
    Ready,
    Todo,
    InProgress,
    Review,
    Done,
    #[serde(other)]
    Other,
}

impl Status {
    pub(crate) const LANES: [Self; 6] = [
        Self::Backlog,
        Self::Ready,
        Self::Todo,
        Self::InProgress,
        Self::Review,
        Self::Done,
    ];

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Backlog => "Backlog",
            Self::Ready => "Ready",
            Self::Todo => "Todo",
            Self::InProgress => "In progress",
            Self::Review => "Review",
            Self::Done => "Done",
            Self::Other => "Other",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum Priority {
    Critical,
    High,
    Medium,
    Low,
    None,
    #[serde(other)]
    Other,
}

impl Priority {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Critical => "Critical",
            Self::High => "High",
            Self::Medium => "Medium",
            Self::Low => "Low",
            Self::None | Self::Other => "No priority",
        }
    }

    /// The bars the card's glyph fills, out of four.
    pub(crate) fn bars(self) -> usize {
        match self {
            Self::Critical => 4,
            Self::High => 3,
            Self::Medium => 2,
            Self::Low => 1,
            Self::None | Self::Other => 0,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub(crate) struct Project {
    pub(crate) id: i64,
    pub(crate) name: String,
    pub(crate) prefix: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub(crate) struct Pull {
    pub(crate) number: i64,
    pub(crate) url: String,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Ticket {
    pub(crate) number: i64,
    pub(crate) project_id: i64,
    pub(crate) parent: Option<i64>,
    pub(crate) title: String,
    pub(crate) description: String,
    pub(crate) status: Status,
    pub(crate) priority: Priority,
    /// `bug`, `feature`, `task`, `epic`, or `chore`, shown as written.
    pub(crate) kind: String,
    /// Empty when unset.
    pub(crate) size: String,
    /// Empty for no one.
    pub(crate) assignee: String,
    /// Empty for the branch herdr-dog derives.
    pub(crate) branch: String,
    pub(crate) labels: Vec<String>,
    pub(crate) pull: Option<Pull>,
    pub(crate) sort_key: f64,
    pub(crate) updated_at: f64,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub(crate) struct Relation {
    #[serde(rename = "type")]
    pub(crate) kind: String,
    #[serde(rename = "ref")]
    pub(crate) reference: String,
    pub(crate) title: String,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Detail {
    pub(crate) ticket: Ticket,
    pub(crate) sub_tickets: Vec<Ticket>,
    pub(crate) relations: Vec<Relation>,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub(crate) enum Author {
    Operator,
    Agent { name: String },
}

impl Author {
    pub(crate) fn name(&self) -> &str {
        match self {
            Self::Operator => "You",
            Self::Agent { name } => name,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Comment {
    pub(crate) id: i64,
    pub(crate) reply_to: Option<i64>,
    /// The id of the thread's first comment.
    pub(crate) thread: i64,
    pub(crate) author: Author,
    pub(crate) body: String,
    pub(crate) created_at: f64,
}

/// Where a ticket's start stands: the agent run herdr-dog began for it in
/// a worktree of its own.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Start {
    /// `creating-worktree`, `starting-agent`, `prompted`, or `failed`.
    pub(crate) stage: String,
    pub(crate) branch: String,
    /// The worktree's absolute path once it exists, or `ssh:HOST:PATH` on
    /// another machine.
    pub(crate) worktree_key: Option<String>,
    pub(crate) error: Option<String>,
}

impl Start {
    /// The worktree's folder on this machine, if it has one here.
    pub(crate) fn local_worktree(&self) -> Option<&str> {
        self.worktree_key
            .as_deref()
            .filter(|key| !key.starts_with("ssh:") && key.starts_with('/'))
    }
}

/// A ticket to create, as herdr-dog's `POST /api/tickets` takes it.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NewTicket {
    pub(crate) project_id: i64,
    pub(crate) title: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub(crate) description: String,
    pub(crate) status: Status,
}

/// herdr-dog's answer envelope: `{ok: true, data}` or `{ok: false, error}`.
/// `data` may be `null`, as for a ticket never started.
#[derive(Deserialize)]
struct Envelope {
    ok: bool,
    #[serde(default)]
    data: serde_json::Value,
    error: Option<Refusal>,
}

#[derive(Deserialize)]
struct Refusal {
    code: String,
    message: String,
}

/// The server at `base`, which answers under `/api`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Server {
    base: WebUrl,
}

impl Server {
    pub(crate) fn new(base: WebUrl) -> Self {
        Self { base }
    }

    /// The server's address, which relative links in its bodies resolve
    /// against.
    pub(crate) fn base(&self) -> &WebUrl {
        &self.base
    }

    pub(crate) fn projects(&self) -> Result<Vec<Project>> {
        self.get("api/tickets/projects")
    }

    /// One project's tickets, sub-tickets included, as the board lists them.
    pub(crate) fn tickets(&self, project: i64) -> Result<Vec<Ticket>> {
        self.get(&format!("api/tickets?project={project}"))
    }

    pub(crate) fn detail(&self, number: i64) -> Result<Detail> {
        self.get(&format!("api/tickets/{number}"))
    }

    /// A ticket's comments, oldest first.
    pub(crate) fn comments(&self, number: i64) -> Result<Vec<Comment>> {
        self.get(&format!("api/tickets/{number}/comments"))
    }

    /// The ticket's start, or `None` while it was never started.
    pub(crate) fn start(&self, number: i64) -> Result<Option<Start>> {
        self.get(&format!("api/tickets/{number}/start"))
    }

    pub(crate) fn create(&self, ticket: &NewTicket) -> Result<Ticket> {
        self.post("api/tickets", ticket)
    }

    /// Adds a comment as the operator, answering `reply_to` when given.
    pub(crate) fn comment(
        &self,
        number: i64,
        body: &str,
        reply_to: Option<i64>,
    ) -> Result<Comment> {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct NewComment<'a> {
            body: &'a str,
            #[serde(skip_serializing_if = "Option::is_none")]
            reply_to: Option<i64>,
        }
        self.post(
            &format!("api/tickets/{number}/comments"),
            &NewComment { body, reply_to },
        )
    }

    /// Moves a ticket to the end of `status`'s lane.
    pub(crate) fn set_status(&self, number: i64, status: Status) -> Result<Ticket> {
        #[derive(Serialize)]
        struct Move {
            status: Status,
        }
        self.post(&format!("api/tickets/{number}/move"), &Move { status })
    }

    fn url(&self, path: &str) -> Result<url::Url> {
        url::Url::parse(self.base.as_str())
            .and_then(|base| base.join(path))
            .map_err(|source| Error::Address {
                path: path.to_owned(),
                source,
            })
    }

    fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        let url = self.url(path)?;
        let reply = agent().get(url.as_str()).call();
        read(reply)
    }

    fn post<T: DeserializeOwned>(&self, path: &str, body: &impl Serialize) -> Result<T> {
        let url = self.url(path)?;
        let body = serde_json::to_vec(body).map_err(Error::Json)?;
        let reply = agent()
            .post(url.as_str())
            .header("Content-Type", "application/json")
            .send(&body[..]);
        read(reply)
    }
}

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(TIMEOUT))
        .max_redirects(0)
        .http_status_as_error(false)
        .build()
        .into()
}

fn read<T: DeserializeOwned>(
    reply: std::result::Result<ureq::http::Response<ureq::Body>, ureq::Error>,
) -> Result<T> {
    let mut reply = reply.map_err(|error| Error::Http(Box::new(error)))?;
    let status = reply.status().as_u16();
    let mut bytes = Vec::new();
    reply
        .body_mut()
        .as_reader()
        .take(LIMIT + 1)
        .read_to_end(&mut bytes)
        .map_err(Error::Read)?;
    interpret(status, &bytes)
}

/// What an answer with `status` and `body` means: its data, or why
/// herdr-dog refused.
pub(super) fn interpret<T: DeserializeOwned>(status: u16, body: &[u8]) -> Result<T> {
    if body.len() as u64 > LIMIT {
        return Err(Error::TooLarge { limit: LIMIT });
    }
    let envelope: Envelope = serde_json::from_slice(body).map_err(Error::Json)?;
    match envelope {
        Envelope { ok: true, data, .. } => serde_json::from_value(data).map_err(Error::Json),
        Envelope {
            error: Some(error), ..
        } => Err(Error::Refused {
            status,
            code: bounded(&error.code),
            message: bounded(&error.message),
        }),
        Envelope { .. } => Err(Error::Json(serde::de::Error::custom(
            "a refusal without its reason",
        ))),
    }
}

/// `text` as one bounded line, safe to show.
fn bounded(text: &str) -> String {
    crate::notifications::safe_text(text, MESSAGE * 4)
        .chars()
        .take(MESSAGE)
        .collect()
}
