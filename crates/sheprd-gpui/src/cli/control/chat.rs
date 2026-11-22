//! `chat`: the native views of `herdr-acp` chats over their panes.
use super::{Args, CliError, Switch, unknown};
use std::ffi::OsString;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChatCommand {
    /// The window's chat panes and how each shows.
    Status,
    /// Open a tab running a chat with `agent`, else the caller's own
    /// chat's agent with that chat's settings.
    New {
        agent: Option<sheprd_chat_protocol::Agent>,
        /// The pane whose workspace and folder it starts in, else the caller's.
        pane: Option<String>,
        /// A brief it starts with, as another agent's first prompt.
        brief: Option<BriefSource>,
    },
    /// Move a pane's terminal agent into a chat there.
    Convert {
        /// The pane, else the caller's own.
        pane: Option<String>,
    },
    /// The person's text shortcuts, which every chat's composer offers.
    Shortcuts,
    /// Star or unstar a pane's agent.
    Favorite {
        switch: Switch,
        /// The pane, else the caller's own.
        pane: Option<String>,
    },
    /// The pull requests the caller's chat lists in its rail, as the agent
    /// of that chat.
    PullRequests(ChatPrCommand),
    /// Show the chat view over a pane (`On`), or its terminal (`Off`).
    View {
        switch: Switch,
        /// The pane, else the caller's own.
        pane: Option<String>,
    },
}

/// Where `chat new` reads its brief from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BriefSource {
    /// `--prompt TEXT`.
    Text(String),
    /// `--prompt-file PATH`; `-` is standard input.
    File(OsString),
}

/// `--prompt` or `--prompt-file`, at most one of them.
fn brief(args: &mut Args) -> Result<Option<BriefSource>, CliError> {
    let text = args.text_option("--prompt")?;
    let file = args.option("--prompt-file")?;
    match (text, file) {
        (Some(_), Some(_)) => Err(CliError::Conflicting("--prompt", "--prompt-file")),
        (Some(text), None) => Ok(Some(BriefSource::Text(text))),
        (None, Some(file)) => Ok(Some(BriefSource::File(file))),
        (None, None) => Ok(None),
    }
}

/// `chat pr`: read or add to the pull requests the caller's chat lists.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChatPrCommand {
    List,
    /// Add the pull request at `url`, one the chat did not see `gh` change.
    Add {
        url: String,
    },
}

pub(super) fn chat(args: &mut Args) -> Result<ChatCommand, CliError> {
    let Some(verb) = args.verb() else {
        return Ok(ChatCommand::Status);
    };
    let switch = match verb.to_str() {
        Some("status" | "list") => return Ok(ChatCommand::Status),
        Some("shortcuts") => return Ok(ChatCommand::Shortcuts),
        Some("pr") => return pull_requests(args).map(ChatCommand::PullRequests),
        Some("new") => {
            let pane = args.text_option("--pane")?;
            let brief = brief(args)?;
            let agent = args
                .optional_text()?
                .map(|agent| agent.parse())
                .transpose()
                .map_err(|_| CliError::InvalidValue("AGENT"))?;
            return Ok(ChatCommand::New { agent, pane, brief });
        }
        Some("favorite") => {
            // The pane first, as it may come before the switch.
            let pane = args.text_option("--pane")?;
            let switch = match args.verb() {
                None => Switch::Toggle,
                Some(word) => match word.to_str() {
                    Some("on") => Switch::On,
                    Some("off") => Switch::Off,
                    Some("toggle") => Switch::Toggle,
                    _ => return Err(unknown("chat favorite", word)),
                },
            };
            return Ok(ChatCommand::Favorite { switch, pane });
        }
        Some("convert") => {
            return Ok(ChatCommand::Convert {
                pane: args.text_option("--pane")?,
            });
        }
        Some("show") => Switch::On,
        Some("hide") => Switch::Off,
        Some("toggle") => Switch::Toggle,
        _ => return Err(unknown("chat", verb)),
    };
    Ok(ChatCommand::View {
        switch,
        pane: args.text_option("--pane")?,
    })
}

fn pull_requests(args: &mut Args) -> Result<ChatPrCommand, CliError> {
    let Some(verb) = args.verb() else {
        return Ok(ChatPrCommand::List);
    };
    match verb.to_str() {
        Some("list") => Ok(ChatPrCommand::List),
        Some("add") => {
            let url = args.required_text("a pull request URL")?;
            // Refused here, before anything reaches the chat.
            sheprd_chat_protocol::PullRequest::parse(url.trim())
                .ok_or(CliError::InvalidValue("URL"))?;
            Ok(ChatPrCommand::Add { url })
        }
        _ => Err(unknown("chat pr", verb)),
    }
}
