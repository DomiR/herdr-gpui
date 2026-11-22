//! The API the native iOS app reads (`docs/ios-app-plan.md`, "The v2 wire,
//! exactly"): home and a chat as JSON, and one WebSocket per phone that
//! follows the views it subscribes to. Commands keep the web app's `POST`
//! routes, so one rate limit and `Origin` policy covers every write.

pub(crate) mod changes;
mod convert;
mod create;
pub(crate) mod github;
mod live;
mod model;
pub(crate) mod previews;
mod search;
mod summarize;
mod title;
mod wire;

pub(crate) use convert::convert;
pub(crate) use create::{create_workspace, new_options, new_terminal, start_chat};
pub(crate) use search::search;
pub(crate) use summarize::summarize;
pub(crate) use title::title;

use crate::{
    control::DeviceId,
    hub::Summary,
    web::{ApiError, Connected, Device, Shared, bearer},
};
use axum::{
    Json,
    extract::{
        Path, State,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    http::HeaderMap,
    response::Response,
};
use model::{ChatSnapshot, Home, Sources};
use secrecy::SecretString;
use std::{
    collections::HashMap,
    time::{Duration, Instant},
};
use tokio::sync::mpsc;
use wire::{ClientMessage, ServerMessage};

/// The largest message a phone may send; a bigger one closes the socket.
const MAX_INBOUND: usize = 4 * 1024;
/// Messages queued for a slow phone; a chat that falls further behind its
/// own queue is told to reload.
const OUTBOUND_QUEUE: usize = 64;
/// Messages from the phone waiting to be handled.
const INBOUND_QUEUE: usize = 16;
/// Streams and sockets one device may hold open at once.
const MAX_STREAMS: usize = 8;
/// How often an idle socket is pinged, so proxies and NATs keep it.
const PING_INTERVAL: Duration = Duration::from_secs(20);
/// How long a socket may go without a frame from the phone, which
/// answers every ping, before it is closed as a path that stopped
/// carrying traffic: two pings unanswered, and some room for a slow pong.
const QUIET_LIMIT: Duration = Duration::from_secs(45);
/// How long one write may wait for room to send. A phone whose path went
/// quiet stops taking bytes, and a write to it would otherwise wait for
/// the operating system to give up, keeping the socket and its views
/// alive past [`QUIET_LIMIT`].
const WRITE_LIMIT: Duration = Duration::from_secs(10);

/// This computer's home as the state holds it now, for `device`.
fn home(state: &Shared, device: &DeviceId) -> Home {
    let layout = state.hub.layout();
    let summaries: HashMap<String, Summary> = layout
        .chats()
        .filter_map(|chat| {
            state
                .hub
                .chat(&chat.pane)
                .map(|link| (chat.pane.clone(), link.summary()))
        })
        .collect();
    let pull_requests = state.pull_requests.borrow().clone();
    let checkouts = state.lending.checkouts.current();
    let recognized = state.lending.recognized();
    let favorites = state.favorites.current();
    let mut home = Home::project(Sources {
        checkouts: &checkouts,
        computer: &state.computer,
        favorites: &favorites,
        fingerprint: &state.fingerprint,
        layout: &layout,
        load: *state.load.borrow(),
        summaries,
        pull_requests: &pull_requests,
        recognized: &recognized,
        shortcuts_ms: state.shortcuts.current().updated_ms,
        tags: state.tags.current(),
        version: crate::server::version(),
        waiting: state.attention().len(),
    });
    home.preview_terminals(&state.previews.current());
    home.github = Some(state.github.enabled());
    home.github_hosts = github::lent_hosts(state, device);
    home
}

/// `GET /api/v2/home`.
pub(crate) async fn get_home(State(state): State<Shared>, Device(device): Device) -> Json<Home> {
    state.hub.wake_for_phone();
    state.lending.checkouts.want();
    Json(home(&state, &device))
}

/// `GET /api/v2/chat/{pane}`: the transcript and its cursor.
pub(crate) async fn get_chat(
    State(state): State<Shared>,
    _device: Device,
    Path(pane): Path<String>,
) -> Result<Json<ChatSnapshot>, ApiError> {
    let link = match state.hub.layout().chat(&pane) {
        Some(_) => state.hub.chat(&pane).ok_or(ApiError::NotFound)?,
        None if state.hub.observable(&pane) => state
            .hub
            .observe(&pane)
            .await
            .map_err(|_| ApiError::NotFound)?,
        None => return Err(ApiError::NotFound),
    };
    let (transcript, cursor) = link.base();
    Ok(Json(ChatSnapshot {
        pane,
        cursor,
        connected: link.summary().connected,
        transcript,
    }))
}

/// `GET /api/v2/live`: upgrades to the socket, unless the device holds
/// [`MAX_STREAMS`] open already.
pub(crate) async fn live(
    State(state): State<Shared>,
    Device(device): Device,
    headers: HeaderMap,
    upgrade: WebSocketUpgrade,
) -> Result<Response, ApiError> {
    let open = state.connected().get(&device).copied().unwrap_or(0);
    if open >= MAX_STREAMS {
        return Err(ApiError::TooManyStreams);
    }
    // `Device` accepted this token, so it is present.
    let token = SecretString::from(bearer(&headers).unwrap_or_default());
    Ok(upgrade
        .max_message_size(MAX_INBOUND)
        .max_frame_size(MAX_INBOUND)
        .on_upgrade(move |socket| serve(socket, state, device, token)))
}

/// Carries messages between the socket and the session until either ends:
/// the phone closes, sends a binary or oversized frame, or the session
/// says `bye`, or the phone stays quiet past [`QUIET_LIMIT`] or takes no
/// bytes for [`WRITE_LIMIT`]. Logs how it ended and how long the phone
/// had been quiet.
async fn serve(mut socket: WebSocket, state: Shared, device: DeviceId, token: SecretString) {
    let _connected = Connected::new(state.clone(), device.clone());
    let (to_phone, mut outbound) = mpsc::channel(OUTBOUND_QUEUE);
    let (inbound, from_phone) = mpsc::channel(INBOUND_QUEUE);
    let session = tokio::spawn(live::run(state, token, device, from_phone, to_phone));
    let mut ping = tokio::time::interval(PING_INTERVAL);
    ping.reset();
    let opened = Instant::now();
    let mut heard = opened;
    let ending = loop {
        tokio::select! {
            message = outbound.recv() => {
                let Some(message) = message else { break Ending::SessionEnded };
                let bye = matches!(message, ServerMessage::Bye { .. });
                if let Some(ending) = send(&mut socket, &message).await {
                    break ending;
                }
                if bye {
                    break Ending::Bye;
                }
            }
            frame = socket.recv() => {
                if matches!(frame, Some(Ok(_))) {
                    heard = Instant::now();
                }
                match frame {
                    Some(Ok(Message::Text(text))) => {
                        let reply = match serde_json::from_str::<ClientMessage>(&text) {
                            Ok(message) => inbound
                                .try_send(message)
                                .err()
                                .map(|_| "too many messages at once"),
                            Err(_) => Some("that message is not one this socket reads"),
                        };
                        if let Some(message) = reply {
                            let error = ServerMessage::Error { message: message.to_owned(), view: None };
                            if let Some(ending) = send(&mut socket, &error).await {
                                break ending;
                            }
                        }
                    }
                    Some(Ok(Message::Ping(_) | Message::Pong(_))) => {}
                    Some(Ok(Message::Close(_))) | None => break Ending::PhoneClosed,
                    Some(Ok(Message::Binary(_))) => break Ending::Unreadable,
                    Some(Err(_)) => break Ending::ReadFailed,
                }
            },
            _ = ping.tick() => {
                if too_quiet(heard.elapsed()) {
                    break Ending::Quiet;
                }
                if let Some(ending) = bounded(WRITE_LIMIT, socket.send(Message::Ping(Default::default()))).await {
                    break ending;
                }
            }
        }
    };
    session.abort();
    let _ = bounded(WRITE_LIMIT, socket.send(Message::Close(None))).await;
    tracing::info!(
        ?ending,
        lasted_s = opened.elapsed().as_secs(),
        quiet_s = heard.elapsed().as_secs(),
        "a phone's live socket ended"
    );
}

/// Why a live socket ended, as its log line says.
#[derive(Debug)]
enum Ending {
    Bye,
    PhoneClosed,
    Quiet,
    ReadFailed,
    SendFailed,
    SessionEnded,
    Stalled,
    Unreadable,
}

/// Whether a phone quiet for `quiet` has stopped answering pings.
fn too_quiet(quiet: Duration) -> bool {
    quiet > QUIET_LIMIT
}

/// Sends `message`; how the socket ends when it cannot.
async fn send(socket: &mut WebSocket, message: &ServerMessage) -> Option<Ending> {
    let Ok(text) = serde_json::to_string(message) else {
        return Some(Ending::SendFailed);
    };
    bounded(WRITE_LIMIT, socket.send(Message::Text(text.into()))).await
}

/// Waits up to `limit` for `write`; how the socket ends when it fails or
/// is still waiting then, nothing when it went out.
async fn bounded<E>(limit: Duration, write: impl Future<Output = Result<(), E>>) -> Option<Ending> {
    match tokio::time::timeout(limit, write).await {
        Ok(Ok(())) => None,
        Ok(Err(_)) => Some(Ending::SendFailed),
        Err(_) => Some(Ending::Stalled),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests;
