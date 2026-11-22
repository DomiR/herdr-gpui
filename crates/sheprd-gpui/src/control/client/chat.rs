//! `chat`: list the window's chat panes, or show a chat view or its terminal.
use super::{Context, Output, unexpected};
use crate::{
    chat::pull_requests,
    cli::control::{ChatCommand, ChatPrCommand},
    control::protocol::{
        Request, Response,
        chat::{ChatInfo, ChatOp, Reply, ShortcutInfo},
    },
};
use std::fmt::Write as _;

/// One chat pane as a line of text.
pub(super) fn describe(chat: &ChatInfo) -> String {
    let mut line = format!(
        "{}  {}  {}{}  {}",
        chat.pane_id,
        if chat.agent.is_empty() {
            "agent"
        } else {
            &chat.agent
        },
        chat.status,
        if chat.attached { "" } else { " (reconnecting)" },
        if chat.shown { "chat view" } else { "terminal" },
    );
    if let Some(model) = &chat.model {
        let _ = write!(line, "  {model}");
    }
    if let Some(context) = chat.context {
        let _ = write!(line, "  {context}% context");
    }
    if chat.permissions > 0 {
        let _ = write!(line, "  {} waiting for permission", chat.permissions);
    }
    if chat.queued > 0 {
        let _ = write!(line, "  {} queued", chat.queued);
    }
    line
}

pub(super) fn chat(command: ChatCommand, context: &Context) -> crate::Result<Output> {
    let op = match command {
        ChatCommand::Status => ChatOp::Status,
        ChatCommand::New { agent, pane, brief } => ChatOp::New {
            agent: agent.map(|agent| agent.name().to_owned()),
            pane,
            prompt: brief.map(crate::control::brief::read).transpose()?,
        },
        ChatCommand::Convert { pane } => ChatOp::Convert { pane },
        ChatCommand::View { switch, pane } => ChatOp::View { switch, pane },
        ChatCommand::Shortcuts => ChatOp::Shortcuts,
        ChatCommand::Favorite { switch, pane } => ChatOp::Favorite { switch, pane },
        // Answered by the caller's own chat, not the app.
        ChatCommand::PullRequests(command) => return chat_pull_requests(command),
    };
    match context.call(Request::Chat, op)? {
        Response::Chat {
            reply: Reply::Chats { chats },
        } => {
            let json = serde_json::json!({ "chats": chats });
            if chats.is_empty() {
                return Ok(Output::data("No chat panes", &json)?.nothing());
            }
            let text: Vec<String> = chats.iter().map(describe).collect();
            Output::data(text.join("\n"), &json)
        }
        Response::Chat {
            reply: Reply::View { pane_id, shown },
        } => Output::data(
            if shown {
                format!("{pane_id} shows its chat view")
            } else {
                format!("{pane_id} shows its terminal")
            },
            &serde_json::json!({ "pane_id": pane_id, "shown": shown }),
        ),
        Response::Chat {
            reply: Reply::Favorite { pane_id, favorite },
        } => Output::data(
            if favorite {
                format!("{pane_id} is a favorite")
            } else {
                format!("{pane_id} is not a favorite")
            },
            &serde_json::json!({ "pane_id": pane_id, "favorite": favorite }),
        ),
        Response::Chat {
            reply: Reply::Started { agent },
        } => Output::data(
            format!("Starting a {agent} chat"),
            &serde_json::json!({ "agent": agent, "started": true }),
        ),
        Response::Chat {
            reply: Reply::Converting { agent, pane_id },
        } => Output::data(
            format!("Moving {agent} in {pane_id} into a chat"),
            &serde_json::json!({ "agent": agent, "pane_id": pane_id, "converting": true }),
        ),
        Response::Chat {
            reply:
                Reply::Shortcuts {
                    shortcuts,
                    updated_ms,
                    complete,
                },
        } => {
            let json = serde_json::json!({
                "shortcuts": shortcuts,
                "updated_ms": updated_ms,
                "complete": complete,
            });
            if shortcuts.is_empty() {
                return Ok(Output::data("No text shortcuts", &json)?.nothing());
            }
            Output::data(describe_shortcuts(&shortcuts, complete), &json)
        }
        response => Err(unexpected(response)),
    }
}

/// The shortcuts as text: each label, then its text indented beneath it.
pub(super) fn describe_shortcuts(shortcuts: &[ShortcutInfo], complete: bool) -> String {
    let mut text = String::new();
    for shortcut in shortcuts {
        let _ = writeln!(text, "{}", shortcut.label);
        for line in shortcut.text.lines() {
            let _ = writeln!(text, "    {line}");
        }
    }
    if !complete {
        text.push_str("(more shortcuts than one answer holds)\n");
    }
    text.trim_end().to_owned()
}

/// `chat pr`, against the chat the caller is the agent of.
fn chat_pull_requests(command: ChatPrCommand) -> crate::Result<Output> {
    let env = std::env::var_os(sheprd_chat_protocol::AGENT_CHAT_ENV);
    let chat = pull_requests::address(env.as_deref())?;
    match command {
        ChatPrCommand::List => {
            let pulls = pull_requests::list(&chat)?;
            let json = serde_json::json!({ "pull_requests": pulls });
            if pulls.is_empty() {
                return Ok(Output::data("No pull requests in this chat", &json)?.nothing());
            }
            Output::data(describe_pull_requests(&pulls), &json)
        }
        ChatPrCommand::Add { url } => {
            let pull = pull_requests::add(&chat, &url)?;
            Output::data(
                format!("{} shows in this chat's rail", pull.url),
                &serde_json::json!({ "added": true, "pull_request": pull }),
            )
        }
    }
}

/// One pull request a line: its number, repository, address, and what the
/// agent did to it.
pub(super) fn describe_pull_requests(pulls: &[sheprd_chat_protocol::PullRequest]) -> String {
    let lines: Vec<String> = pulls
        .iter()
        .map(|pull| {
            let mut line = format!("#{}  {}  {}", pull.number, pull.repo, pull.url);
            if !pull.actions.is_empty() {
                let actions: Vec<String> = pull
                    .actions
                    .iter()
                    .filter_map(|action| serde_json::to_value(action).ok())
                    .filter_map(|value| value.as_str().map(str::to_owned))
                    .collect();
                let _ = write!(line, "  {}", actions.join(", "));
            }
            line
        })
        .collect();
    lines.join("\n")
}
