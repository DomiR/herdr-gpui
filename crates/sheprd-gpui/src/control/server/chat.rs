//! `chat status`, `chat shortcuts`, `chat favorite`, `chat show`, `chat hide`,
//! and `chat toggle`.
use super::{Watches, in_window, not_found, socket};
use crate::chat::brief::Brief;
use crate::control::protocol::{
    Call, Caller, ErrorCode, Response,
    chat::{ChatOp, MAX_BRIEF, Reply, valid_brief},
};
use gpui::App;
use std::time::Duration;

/// How long writing a brief's file may take before `chat new` gives up.
const WRITE_WAIT: Duration = Duration::from_secs(10);

pub(super) fn serve(call: Call<ChatOp>, cx: &mut App) -> Response {
    let Call { caller, op } = call;
    match op {
        ChatOp::Status => in_window(&caller, cx, |view, _, _| Response::Chat {
            reply: Reply::Chats {
                chats: info::infos(view),
            },
        }),
        ChatOp::New {
            agent,
            pane,
            prompt: None,
        } => start_new(&caller, agent, pane, None, cx),
        // Parked by `serve_parked` until its file is written.
        ChatOp::New {
            prompt: Some(_), ..
        } => Response::error(
            ErrorCode::Failed,
            "A brief is answered once its file is written",
        ),
        ChatOp::Convert { pane } => {
            let pane = pane.or_else(|| callers_pane(&caller));
            in_window(&caller, cx, move |view, _, cx| {
                match view.convert_to_chat(pane.as_deref()) {
                    Ok((pane_id, agent)) => {
                        cx.notify();
                        Response::Chat {
                            reply: Reply::Converting {
                                agent: agent.name().into(),
                                pane_id,
                            },
                        }
                    }
                    Err(error) => Response::error(convert_code(&error), error.to_string()),
                }
            })
        }
        ChatOp::Shortcuts => in_window(&caller, cx, |_, _, cx| {
            let list = crate::text_shortcuts::Store::current(cx);
            let (shortcuts, complete) =
                crate::control::protocol::chat::bounded_shortcuts(list.shortcuts.iter().map(
                    |shortcut| crate::control::protocol::chat::ShortcutInfo {
                        id: shortcut.id.clone(),
                        label: shortcut.label(),
                        title: shortcut.title.clone(),
                        text: shortcut.text.clone(),
                    },
                ));
            Response::Chat {
                reply: Reply::Shortcuts {
                    shortcuts,
                    updated_ms: list.updated_ms,
                    complete,
                },
            }
        }),
        ChatOp::Favorite { switch, pane } => in_window(&caller, cx, |view, window, cx| {
            // A focused herdr-core terminal counts without a chat view: it
            // is nobody's Herdr pane.
            let pane = pane
                .or_else(|| callers_pane(&caller))
                .or_else(|| view.focused_core_key(window, cx))
                .or_else(|| {
                    view.live
                        .snapshot
                        .as_ref()
                        .and_then(|snapshot| snapshot.focused_pane_id.clone())
                });
            let Some(pane) = pane else {
                return Response::error(ErrorCode::InvalidRequest, "Name a pane with --pane");
            };
            let Some(starred) = view.starred_pane(&pane) else {
                return not_found(format!("Pane {pane} runs no agent this window can star"));
            };
            let favorite = switch.apply(starred.favorite);
            if favorite != starred.favorite {
                crate::agent_favorites::AgentFavorites::set(
                    cx,
                    &starred.endpoint,
                    starred.key,
                    favorite,
                );
            }
            Response::Chat {
                reply: Reply::Favorite {
                    pane_id: pane,
                    favorite,
                },
            }
        }),
        ChatOp::View { switch, pane } => in_window(&caller, cx, |view, window, cx| {
            // A herdr-core pane is nobody's Herdr pane: the one holding the
            // keyboard comes before Herdr's focused pane.
            let pane = pane
                .or_else(|| callers_pane(&caller))
                .or_else(|| view.focused_core_chat_key(window, cx))
                .or_else(|| {
                    view.live
                        .snapshot
                        .as_ref()
                        .and_then(|snapshot| snapshot.focused_pane_id.clone())
                });
            let Some(pane) = pane else {
                return Response::error(ErrorCode::InvalidRequest, "Name a pane with --pane");
            };
            if !view.chats.pane(&pane).is_some_and(|chat| chat.available()) {
                return not_found(format!("Pane {pane} has no chat this window can show"));
            }
            let shown = switch.apply(view.chats.shown(&pane));
            view.set_chat_raw(&pane, !shown, window, cx);
            Response::Chat {
                reply: Reply::View {
                    pane_id: pane,
                    shown,
                },
            }
        }),
    }
}

/// `chat new`: a tab running a chat, started with `brief` when it has one.
/// A brief that does not reach a program is deleted.
fn start_new(
    caller: &Caller,
    agent: Option<String>,
    pane: Option<String>,
    mut brief: Option<Brief>,
    cx: &mut App,
) -> Response {
    let named = match agent.map(|agent| agent.parse::<sheprd_chat_protocol::Agent>()) {
        None => None,
        Some(Ok(agent)) => Some(agent),
        Some(Err(_)) => {
            discard(brief, cx);
            return Response::error(
                ErrorCode::InvalidRequest,
                format!(
                    "Unknown agent; known agents are {}",
                    sheprd_chat_protocol::Agent::names()
                ),
            );
        }
    };
    let own = callers_pane(caller);
    let beside = pane.or_else(|| caller.pane_id.clone());
    let kept = brief.clone();
    let response = in_window(caller, cx, move |view, _, cx| {
        // A chat's agent asking for another chat gets one like its
        // own: the same agent, unless it names another, and then
        // the same settings too.
        let like = own.as_deref().and_then(|own| view.like_chat(own));
        let Some(agent) = named.or(like.as_ref().map(|(agent, _)| *agent)) else {
            return Response::error(
                ErrorCode::InvalidRequest,
                format!(
                    "Name an agent ({}); only a chat's own agent may leave it out",
                    sheprd_chat_protocol::Agent::names()
                ),
            );
        };
        // The brief is the asking chat's agent's, whatever agent it starts.
        if let Some(brief) = &mut brief {
            brief.from = like.as_ref().map(|(agent, _)| *agent);
        }
        let carry = like
            .as_ref()
            .filter(|(own, _)| *own == agent)
            .map(|(_, settings)| settings);
        // With no Herdr pane named, as from an agent in a herdr-core
        // pane, a herdr-core tab in the active group splits instead.
        let core = beside
            .is_none()
            .then(|| view.core_chat_group_tab(None, cx))
            .flatten();
        view.chats.brief = brief;
        let started = if let Some(key) = beside
            .as_deref()
            .filter(|key| key.starts_with("core:") || key.starts_with("core@"))
        {
            view.start_core_chat_beside(agent, key, carry, cx)
        } else {
            match core {
                Some(tab) => view.start_core_chat(agent, tab, carry, cx),
                None if beside.is_none() && view.kept_in_front().is_some() => {
                    view.start_kept_core_chat(agent, carry, cx)
                }
                None => view.start_chat(agent, beside.as_deref(), carry),
            }
        };
        view.chats.brief = None;
        match started {
            Ok(()) => {
                cx.notify();
                Response::Chat {
                    reply: Reply::Started {
                        agent: agent.name().into(),
                    },
                }
            }
            Err(error) => Response::error(start_code(&error), error.to_string()),
        }
    });
    if matches!(response, Response::Error { .. }) {
        discard(kept, cx);
    }
    response
}

/// Deletes, off the UI thread, a brief no chat will read.
fn discard(brief: Option<Brief>, cx: &App) {
    if let Some(brief) = brief {
        cx.background_executor()
            .spawn(async move { brief.discard() })
            .detach();
    }
}

/// [`serve`], parking `chat new --prompt` until its brief is written off
/// the UI thread; the chat then starts as without one.
pub(super) fn serve_parked(
    call: Call<ChatOp>,
    incoming: socket::Incoming,
    watches: &mut Watches,
    cx: &mut App,
) {
    let Call { caller, op } = call;
    let ChatOp::New {
        agent,
        pane,
        prompt: Some(prompt),
    } = op
    else {
        incoming.respond(serve(Call { caller, op }, cx));
        return;
    };
    if !valid_brief(&prompt) {
        incoming.respond(Response::error(
            ErrorCode::InvalidRequest,
            format!("A brief is text of at most {MAX_BRIEF} bytes"),
        ));
        return;
    }
    let written = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot = written.clone();
    let task = cx.background_executor().spawn(async move {
        // The program found here is the one the chat runs; an older
        // one would refuse the brief's option and start nothing.
        if crate::chat::start_program().is_some_and(|program| !crate::chat::takes_briefs(&program))
        {
            return Err(crate::chat::brief::Error::Unsupported);
        }
        Brief::write(&prompt, None)
    });
    cx.spawn(async move |cx| {
        let result = task.await;
        cx.update(|_| *slot.borrow_mut() = Some(result));
    })
    .detach();
    let mut request = Some((caller, agent, pane));
    watches.add(
        incoming,
        WRITE_WAIT,
        move |cx| {
            let result = written.borrow_mut().take()?;
            let (caller, agent, pane) = request.take()?;
            Some(match result {
                Ok(brief) => start_new(&caller, agent, pane, Some(brief), cx),
                Err(error @ crate::chat::brief::Error::Unsupported) => {
                    Response::error(ErrorCode::Unsupported, error.to_string())
                }
                Err(error) => Response::error(ErrorCode::Failed, error.to_string()),
            })
        },
        Response::error(ErrorCode::Timeout, "The brief was not written in time"),
        cx,
    );
}

/// The caller's own pane, as chats key it: its Herdr pane, or its
/// herdr-core terminal as `core:t7`, so a caller in one moves its own
/// terminal's agent rather than Herdr 1's focused pane.
fn callers_pane(caller: &Caller) -> Option<String> {
    crate::control::origin(caller.pane_id.as_deref(), caller.terminal_id.as_deref())
        .map(std::borrow::Cow::into_owned)
}

/// How a chat that did not start is reported to the caller.
fn start_code(error: &crate::chat::StartError) -> ErrorCode {
    use crate::chat::StartError;
    match error {
        StartError::AgentMissing(_)
        | StartError::BriefRemote
        | StartError::Disabled
        | StartError::Line
        | StartError::NotLocal
        | StartError::ProgramMissing => ErrorCode::Unsupported,
        StartError::NoWorkspace => ErrorCode::NotFound,
        StartError::SlotBusy | StartError::ProgramUnknown => ErrorCode::Busy,
        StartError::NotConnected
        | StartError::Request(_)
        | StartError::Response
        | StartError::Timeout => ErrorCode::Failed,
    }
}

/// How an agent that did not move is reported to the caller.
fn convert_code(error: &crate::chat::ConvertError) -> ErrorCode {
    use crate::chat::ConvertError;
    use sheprd_client::herdr::convert;
    match error {
        ConvertError::Program(error) => start_code(error),
        ConvertError::Convert(
            convert::Error::Line
            | convert::Error::NoCoreSession(_)
            | convert::Error::NoSession(_)
            | convert::Error::NoShell(_)
            | convert::Error::Unknown(_)
            | convert::Error::Unsupported(_),
        )
        | ConvertError::Disabled
        | ConvertError::HostProgram(_)
        | ConvertError::NoHome
        | ConvertError::NotReachable => ErrorCode::Unsupported,
        ConvertError::Convert(
            convert::Error::AlreadyChat
            | convert::Error::FolderGone(_)
            | convert::Error::NoAgent
            | convert::Error::NotRunning(_),
        ) => ErrorCode::NotFound,
        ConvertError::AlreadyConverting
        | ConvertError::Claimed
        | ConvertError::Convert(convert::Error::Busy(_)) => ErrorCode::Busy,
        ConvertError::Convert(
            convert::Error::Moved | convert::Error::StillRunning(_) | convert::Error::Stopped,
        )
        | ConvertError::Answer(_)
        | ConvertError::Core(_)
        | ConvertError::Existing(_)
        | ConvertError::NotConnected
        | ConvertError::Remote(_)
        | ConvertError::Stale
        | ConvertError::Type(_)
        | ConvertError::Worker(_) => ErrorCode::Failed,
    }
}

mod info {
    use crate::{
        SheprdWindow,
        control::protocol::chat::{ChatInfo, InputMode},
    };
    use sheprd_chat_protocol::Status;

    pub(super) fn infos(view: &SheprdWindow) -> Vec<ChatInfo> {
        view.chats
            .panes()
            .map(|chat| {
                let transcript = chat.session.transcript();
                let mut status = match transcript.status {
                    Status::Starting => "starting",
                    Status::Idle => "idle",
                    Status::Working => "working",
                    Status::Failed { .. } => "failed",
                    Status::Exited { .. } => "exited",
                };
                if chat.target.existing.is_some()
                    && view.live.snapshot.as_ref().is_some_and(|snapshot| {
                        snapshot.agents.iter().any(|agent| {
                            agent.pane_id == chat.target.pane_id
                                && agent.agent_status
                                    == sheprd_client::protocol::AgentStatus::Blocked
                        })
                    })
                {
                    status = "blocked";
                }
                ChatInfo {
                    pane_id: chat.target.pane_id.clone(),
                    agent: chat.session.agent().to_owned(),
                    input_mode: if chat.target.existing.is_some() {
                        InputMode::Terminal
                    } else if transcript.read_only {
                        InputMode::ReadOnly
                    } else {
                        InputMode::Chat
                    },
                    session_id: transcript
                        .session
                        .as_ref()
                        .map(|session| session.id.clone()),
                    status: status.into(),
                    attached: chat.attached(),
                    shown: view.chats.shown(&chat.target.pane_id),
                    permissions: transcript.permissions.len(),
                    queued: transcript.queued.len(),
                    model: transcript
                        .option(sheprd_chat_protocol::OptionCategory::Model)
                        .map(sheprd_chat_protocol::ConfigOption::current_name),
                    context: transcript
                        .usage
                        .as_ref()
                        .filter(|usage| usage.size > 0)
                        .map(sheprd_chat_protocol::Usage::percent),
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests;
