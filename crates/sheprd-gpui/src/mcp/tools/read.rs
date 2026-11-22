//! Tools that only read what Sheprd shows or keeps.

use super::{Effect, Spec, integer, none, string};
use crate::cli::control::{
    BrowserCommand, ChatCommand, CheckpointCommand, ControlCommand, MAX_WAIT, MemoryCommand,
    NotesCommand, PortsCommand, PrCommand, SecurityCommand, TabCommand, WaitFor, WindowCommand,
    WorktreeCommand,
};
use serde_json::json;
use sheprd_memory::MAX_LIMIT;

/// What `memory list` and `memory search` return unless asked.
const MEMORY_LIMIT: u64 = 20;
const MEMORY_INDEX_CHARS: u64 = 3000;
/// The longest search, checkpoint name, and the like.
const SHORT_TEXT: usize = 1024;

pub(super) const TOOLS: &[Spec] = &[
    // keep-sorted start
    Spec {
        name: "browser_feedback",
        title: "Read the user's notes",
        description: "Notes the user sent this agent's Herdr pane from pages, reviews, or terminals, which were not typed into it. Works only when the MCP client runs in a Herdr pane. Waits up to wait_seconds for notes when none are there yet.",
        effect: Effect::Reads,
        properties: || json!({ "wait_seconds": integer("How long to wait for notes; 0 only checks.", 0, MAX_WAIT) }),
        required: &[],
        build: |arguments| {
            Ok(ControlCommand::Feedback {
                wait: arguments.number("wait_seconds", 0..=MAX_WAIT)?.unwrap_or(0),
            })
        },
    },
    Spec {
        name: "browser_list",
        title: "List page tabs",
        description: "The browser tabs Sheprd's window shows, with their ids, addresses, and titles.",
        effect: Effect::Reads,
        properties: none,
        required: &[],
        build: |_| Ok(ControlCommand::Browser(BrowserCommand::List)),
    },
    Spec {
        name: "browser_text",
        title: "Read a page's text",
        description: "The visible text of a browser tab, by the id browser_list reports.",
        effect: Effect::Reads,
        properties: || json!({ "id": integer("The page tab's id.", 0, u64::MAX) }),
        required: &["id"],
        build: |arguments| {
            Ok(ControlCommand::Browser(BrowserCommand::Text {
                id: arguments.required_number("id", 0..=u64::MAX)?,
                focus: false,
            }))
        },
    },
    Spec {
        name: "chat_status",
        title: "List chats",
        description: "The window's sheprd-acp chat panes: agent, status, model, and context window use.",
        effect: Effect::Reads,
        properties: none,
        required: &[],
        build: |_| Ok(ControlCommand::Chat(ChatCommand::Status)),
    },
    Spec {
        name: "checkpoint_diff",
        title: "Diff checkpoints",
        description: "What changed between two checkpoints of the checkout the MCP server runs in, or since one in the working tree.",
        effect: Effect::Reads,
        properties: || {
            json!({
                "from": string("The checkpoint to compare from, as checkpoint_list names it.", SHORT_TEXT),
                "to": string("The checkpoint to compare to; absent, the working tree.", SHORT_TEXT),
            })
        },
        required: &["from"],
        build: |arguments| {
            Ok(ControlCommand::Checkpoint(CheckpointCommand::Diff {
                from: arguments.required_text("from", SHORT_TEXT)?,
                to: arguments.text("to", SHORT_TEXT)?,
            }))
        },
    },
    Spec {
        name: "checkpoint_list",
        title: "List checkpoints",
        description: "Checkpoints of the checkout the MCP server runs in, newest first: snapshots taken as agents start and finish turns.",
        effect: Effect::Reads,
        properties: none,
        required: &[],
        build: |_| Ok(ControlCommand::Checkpoint(CheckpointCommand::List)),
    },
    Spec {
        name: "memory_get",
        title: "Read a memory",
        description: "One entry of the repository's agent memory, with its details.",
        effect: Effect::Reads,
        properties: || json!({ "id": integer("The entry's id.", 1, i64::MAX as u64) }),
        required: &["id"],
        build: |arguments| {
            Ok(ControlCommand::Memory(MemoryCommand::Get {
                id: memory_id(arguments)?,
            }))
        },
    },
    Spec {
        name: "memory_index",
        title: "Read memory to start with",
        description: "What an agent should know before starting work in the repository the MCP server runs in: pinned entries, this branch's, then the most recently used. Read it at the start of a task.",
        effect: Effect::Reads,
        properties: || json!({ "chars": integer("The most characters to return.", 200, 16_000) }),
        required: &[],
        build: |arguments| {
            Ok(ControlCommand::Memory(MemoryCommand::Index {
                chars: arguments
                    .number("chars", 200..=16_000)?
                    .unwrap_or(MEMORY_INDEX_CHARS) as usize,
            }))
        },
    },
    Spec {
        name: "memory_list",
        title: "List memories",
        description: "What agents learned about the repository the MCP server runs in, shared by its worktrees: pinned entries first, then the most recently used.",
        effect: Effect::Reads,
        properties: || {
            json!({
                "all": { "type": "boolean", "description": "Include every branch's entries, not only this branch's." },
                "limit": integer("The most entries to return.", 1, MAX_LIMIT as u64),
            })
        },
        required: &[],
        build: |arguments| {
            Ok(ControlCommand::Memory(MemoryCommand::List {
                all: arguments.flag("all")?,
                limit: memory_limit(arguments)?,
            }))
        },
    },
    Spec {
        name: "memory_search",
        title: "Search memory",
        description: "Entries of the repository's agent memory holding every word (as prefixes), else any of them, best first. Search before starting a task.",
        effect: Effect::Reads,
        properties: || {
            json!({
                "query": string("The words to look for.", SHORT_TEXT),
                "all": { "type": "boolean", "description": "Include every branch's entries, not only this branch's." },
                "limit": integer("The most entries to return.", 1, MAX_LIMIT as u64),
            })
        },
        required: &["query"],
        build: |arguments| {
            Ok(ControlCommand::Memory(MemoryCommand::Search {
                query: arguments.required_text("query", SHORT_TEXT)?,
                all: arguments.flag("all")?,
                limit: memory_limit(arguments)?,
            }))
        },
    },
    Spec {
        name: "notes_get",
        title: "Read the worktree note",
        description: "The note the user keeps on the caller's checkout (its repository and branch).",
        effect: Effect::Reads,
        properties: none,
        required: &[],
        build: |_| Ok(ControlCommand::Notes(NotesCommand::Get)),
    },
    Spec {
        name: "notes_list",
        title: "List worktree notes",
        description: "Every note the user keeps on a worktree.",
        effect: Effect::Reads,
        properties: none,
        required: &[],
        build: |_| Ok(ControlCommand::Notes(NotesCommand::List)),
    },
    Spec {
        name: "ports_list",
        title: "List ports",
        description: "Ports the caller's workspace listens on, and which are forwarded to this computer.",
        effect: Effect::Reads,
        properties: none,
        required: &[],
        build: |_| Ok(ControlCommand::Ports(PortsCommand::List)),
    },
    Spec {
        name: "pr_checks",
        title: "Read pull request checks",
        description: "The CI checks of the caller's branch's pull request, on the window's selected host.",
        effect: Effect::Reads,
        properties: none,
        required: &[],
        build: |_| Ok(ControlCommand::Pr(PrCommand::Checks)),
    },
    Spec {
        name: "pr_comments",
        title: "Read pull request comments",
        description: "Review comments on the caller's branch's pull request. Needs the caller's workspace focused.",
        effect: Effect::Reads,
        properties: none,
        required: &[],
        build: |_| Ok(ControlCommand::Pr(PrCommand::Comments)),
    },
    Spec {
        name: "pr_status",
        title: "Read pull request status",
        description: "The caller's branch's pull request: number, title, state, review, and checks summary.",
        effect: Effect::Reads,
        properties: none,
        required: &[],
        build: |_| Ok(ControlCommand::Pr(PrCommand::Status)),
    },
    Spec {
        name: "security_list",
        title: "List security alerts",
        description: "Private vulnerability reports in triage (titles and metadata only) and open Dependabot alerts of the user's repositories.",
        effect: Effect::Reads,
        properties: none,
        required: &[],
        build: |_| Ok(ControlCommand::Security(SecurityCommand::List)),
    },
    Spec {
        name: "status",
        title: "Sheprd status",
        description: "Whether Sheprd runs, its version, its windows, and their hosts.",
        effect: Effect::Reads,
        properties: none,
        required: &[],
        build: |_| Ok(ControlCommand::Status),
    },
    Spec {
        name: "system",
        title: "Read host load",
        description: "CPU, memory, disk, and uptime of the window's selected host.",
        effect: Effect::Reads,
        properties: none,
        required: &[],
        build: |_| Ok(ControlCommand::System),
    },
    Spec {
        name: "tab_list",
        title: "List Sheprd tabs",
        description: "The app's own tabs (pages, code, reviews, Devices) with their ids.",
        effect: Effect::Reads,
        properties: none,
        required: &[],
        build: |_| Ok(ControlCommand::Tab(TabCommand::List)),
    },
    Spec {
        name: "usage",
        title: "Read plan usage",
        description: "AI plan usage of the window's selected host, per provider.",
        effect: Effect::Reads,
        properties: none,
        required: &[],
        build: |_| Ok(ControlCommand::Usage),
    },
    Spec {
        name: "wait",
        title: "Wait for something",
        description: "Wait until the user sends notes (feedback), a window opens (window), a tab closes (tab_closed, with tab_id), or the host is back online (online).",
        effect: Effect::Reads,
        properties: || {
            json!({
                "for": { "type": "string", "enum": ["feedback", "window", "tab_closed", "online"], "description": "What to wait for." },
                "tab_id": integer("With tab_closed: the tab, by the id tab_list reports.", 0, u64::MAX),
                "timeout_seconds": integer("How long to wait at most (default 60).", 1, MAX_WAIT),
            })
        },
        required: &["for"],
        build: |arguments| {
            let what = match arguments.required_text("for", 16)?.as_str() {
                "feedback" => WaitFor::Feedback,
                "window" => WaitFor::Window,
                "online" => WaitFor::Online,
                "tab_closed" => {
                    WaitFor::TabClosed(arguments.required_number("tab_id", 0..=u64::MAX)?)
                }
                _ => {
                    return Err(super::Error::Choice {
                        name: "for",
                        choices: "feedback, window, tab_closed, online",
                    });
                }
            };
            Ok(ControlCommand::Wait {
                timeout: arguments
                    .number("timeout_seconds", 1..=MAX_WAIT)?
                    .unwrap_or(60),
                what,
            })
        },
    },
    Spec {
        name: "window_alerts",
        title: "Read alerts",
        description: "The alerts the window kept, newest first, including those held back by quiet hours.",
        effect: Effect::Reads,
        properties: none,
        required: &[],
        build: |_| Ok(ControlCommand::Window(WindowCommand::Alerts)),
    },
    Spec {
        name: "window_list",
        title: "List windows",
        description: "Sheprd's windows with their ids, size, workspace, and hosts.",
        effect: Effect::Reads,
        properties: none,
        required: &[],
        build: |_| Ok(ControlCommand::Window(WindowCommand::List)),
    },
    Spec {
        name: "worktree_list",
        title: "List worktrees",
        description: "The Git workspaces Sheprd keeps on the caller's host, with host, path, branch, and linked-checkout status.",
        effect: Effect::Reads,
        properties: none,
        required: &[],
        build: |_| Ok(ControlCommand::Worktree(WorktreeCommand::List)),
    },
    // keep-sorted end
];

fn memory_limit(arguments: &mut super::Arguments) -> Result<usize, super::Error> {
    Ok(arguments
        .number("limit", 1..=MAX_LIMIT as u64)?
        .unwrap_or(MEMORY_LIMIT) as usize)
}

pub(super) fn memory_id(arguments: &mut super::Arguments) -> Result<i64, super::Error> {
    let id = arguments.required_number("id", 1..=i64::MAX as u64)?;
    Ok(i64::try_from(id).unwrap_or(i64::MAX))
}
