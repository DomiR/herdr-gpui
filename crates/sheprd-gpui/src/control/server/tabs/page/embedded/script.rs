//! `browser eval`, `browser logs`, and `browser clear-data`: an agent's
//! own script in a page, the console the page kept, and forgetting what
//! WebKit stores for its site. Each asks the page once, when a window has
//! made it, and answers when WebKit does. A script is never run twice.
use super::{MAX_TEXT_JSON, holder, page_tab};
use crate::{
    browser::{
        TabId,
        page_script::{Error, Outcome},
    },
    control::{
        protocol::{
            Caller, ErrorCode, MAX_WAIT_SECONDS, Response,
            tabs::{ConsoleEntry, MAX_EVAL_SCRIPT_BYTES, PAGE_WAIT_SECONDS, Reply},
        },
        server::{Watches, socket, tabs::reveal},
    },
    window::main_window::MainWindow,
};
use gpui::App;
use serde::Deserialize;
use std::{cell::RefCell, rc::Rc, time::Duration};

/// Where WebKit's answer lands, on the main thread.
type Landed<T> = Rc<RefCell<Option<T>>>;

/// Waits up to `wait` for a window to make tab `id`'s page, then calls
/// `ask` with that window and a slot for the answer, and responds with
/// `answer` of what lands there; `None` from `answer` (nothing ran yet)
/// asks again on the next tick.
#[allow(clippy::too_many_arguments)]
fn ask_once<T: 'static>(
    caller: Caller,
    id: u64,
    wait: Duration,
    expired: &str,
    ask: impl Fn(MainWindow, TabId, Landed<T>, &mut App) -> bool + 'static,
    answer: impl Fn(T) -> Option<Response> + 'static,
    incoming: socket::Incoming,
    watches: &mut Watches,
    cx: &mut App,
) {
    let tab = match page_tab(id, cx) {
        Ok(tab) => tab,
        Err(response) => return incoming.respond(response),
    };
    if holder(&caller, tab.id, cx).is_none()
        && let Err(response) = reveal(&caller, &tab, cx)
    {
        return incoming.respond(response);
    }
    let tab_id = tab.id;
    let landed: Landed<T> = Rc::default();
    let mut asked = false;
    watches.add(
        incoming,
        wait,
        move |cx| {
            if asked {
                let response = answer(landed.borrow_mut().take()?);
                asked = response.is_some();
                return response;
            }
            let handle = holder(&caller, tab_id, cx)?;
            asked = ask(handle, tab_id, landed.clone(), cx);
            None
        },
        Response::error(ErrorCode::Timeout, format!("Tab {id}'s page {expired}")),
        cx,
    );
}

pub(in crate::control::server::tabs) fn eval(
    caller: Caller,
    id: u64,
    script: String,
    wait_seconds: u64,
    incoming: socket::Incoming,
    watches: &mut Watches,
    cx: &mut App,
) {
    if script.trim().is_empty() || script.len() > MAX_EVAL_SCRIPT_BYTES {
        return incoming.respond(Response::error(
            ErrorCode::InvalidRequest,
            format!(
                "The script must be between 1 byte and {} KiB",
                MAX_EVAL_SCRIPT_BYTES / 1024
            ),
        ));
    }
    let wait = wait_seconds.clamp(1, MAX_WAIT_SECONDS);
    ask_once(
        caller,
        id,
        Duration::from_secs(wait),
        &format!("did not finish the script within {wait} seconds"),
        move |handle, tab_id, slot, cx| {
            handle
                .read_with(cx, |view, _| {
                    view.page_eval(tab_id, &script, move |outcome| {
                        *slot.borrow_mut() = Some(outcome);
                    })
                })
                .unwrap_or(false)
        },
        move |outcome| match outcome {
            Ok(Outcome::NotReady) => None,
            outcome => Some(evaluated(id, outcome)),
        },
        incoming,
        watches,
        cx,
    );
}

pub(in crate::control::server::tabs) fn logs(
    caller: Caller,
    id: u64,
    clear: bool,
    incoming: socket::Incoming,
    watches: &mut Watches,
    cx: &mut App,
) {
    ask_once(
        caller,
        id,
        Duration::from_secs(PAGE_WAIT_SECONDS),
        "did not hand back its console in time",
        move |handle, tab_id, slot, cx| {
            handle
                .read_with(cx, |view, _| {
                    view.page_console(tab_id, clear, move |outcome| {
                        *slot.borrow_mut() = Some(outcome);
                    })
                })
                .unwrap_or(false)
        },
        move |outcome| match outcome {
            Ok(Outcome::NotReady) => None,
            outcome => Some(console(id, outcome)),
        },
        incoming,
        watches,
        cx,
    );
}

pub(in crate::control::server::tabs) fn clear_data(
    caller: Caller,
    id: u64,
    incoming: socket::Incoming,
    watches: &mut Watches,
    cx: &mut App,
) {
    ask_once(
        caller,
        id,
        Duration::from_secs(PAGE_WAIT_SECONDS),
        "did not have its site's data cleared in time",
        move |handle, tab_id, slot, cx| {
            handle
                .read_with(cx, |view, _| {
                    view.page_clear_site(tab_id, move |cleared| {
                        *slot.borrow_mut() = Some(cleared);
                    })
                })
                .unwrap_or(false)
        },
        move |cleared| {
            Some(match cleared {
                Ok((host, records)) if records.is_empty() => Response::done(format!(
                    "WebKit kept nothing for {host}; reload tab {id} to load it fresh"
                )),
                Ok((_, records)) => Response::done(format!(
                    "Cleared what WebKit kept for {} (cache, cookies, storage, and service workers, on every port). Reload tab {id} to load it fresh",
                    records.join(", ")
                )),
                Err(error) => failed(id, &error),
            })
        },
        incoming,
        watches,
        cx,
    );
}

fn not_ready(id: u64) -> Response {
    Response::error(
        ErrorCode::NotFound,
        format!("Tab {id}'s page has not loaded yet"),
    )
}

fn failed(id: u64, error: &Error) -> Response {
    Response::error(ErrorCode::Failed, format!("Tab {id}'s page: {error}"))
}

/// The answer to `browser eval`: the value when it fits an answer.
fn evaluated(id: u64, outcome: Result<Outcome, Error>) -> Response {
    match outcome {
        Ok(Outcome::NotReady) => not_ready(id),
        Ok(Outcome::Undefined) => Response::Tab {
            reply: Reply::Eval { value: None },
        },
        Ok(Outcome::Value { value }) => {
            let size = serde_json::to_string(&value).map_or(usize::MAX, |json| json.len());
            if size > MAX_TEXT_JSON {
                return Response::error(
                    ErrorCode::Failed,
                    format!(
                        "Tab {id}'s script returned {size} bytes of JSON; at most {} KiB fit. Return less",
                        MAX_TEXT_JSON / 1024
                    ),
                );
            }
            Response::Tab {
                reply: Reply::Eval { value: Some(value) },
            }
        }
        Ok(Outcome::Threw { message }) => Response::error(
            ErrorCode::Failed,
            format!("Tab {id}'s script threw {message}"),
        ),
        Err(error) => Response::error(
            ErrorCode::Failed,
            format!("Tab {id}'s script did not run: {error}"),
        ),
    }
}

/// What the page's console keeper hands back.
#[derive(Deserialize)]
struct Kept {
    entries: Vec<ConsoleEntry>,
    dropped: u64,
}

/// The answer to `browser logs`: the newest entries that fit an answer,
/// the rest counted as dropped, their text without control characters but
/// line breaks and tabs. The page wrote them, so they are untrusted.
fn console(id: u64, outcome: Result<Outcome, Error>) -> Response {
    let value = match outcome {
        Ok(Outcome::NotReady) => return not_ready(id),
        Ok(Outcome::Value { value }) => value,
        Ok(Outcome::Undefined) => serde_json::Value::Null,
        Ok(Outcome::Threw { message }) => {
            return Response::error(
                ErrorCode::Failed,
                format!("Tab {id}'s console could not be read: {message}"),
            );
        }
        Err(error) => return failed(id, &error),
    };
    if value.is_null() {
        return Response::error(
            ErrorCode::NotFound,
            format!("Tab {id}'s page keeps no console; reload it to start one"),
        );
    }
    let Ok(Kept { entries, dropped }) = serde_json::from_value::<Kept>(value) else {
        return Response::error(
            ErrorCode::Failed,
            format!("Tab {id}'s page handed back a console the app could not read"),
        );
    };
    let total = entries.len();
    let mut room = MAX_TEXT_JSON;
    let mut kept: Vec<ConsoleEntry> = entries
        .into_iter()
        .rev()
        .map(|entry| ConsoleEntry {
            text: printable(&entry.text),
            ..entry
        })
        .take_while(|entry| {
            let size = serde_json::to_string(entry).map_or(usize::MAX, |json| json.len() + 1);
            let fits = size <= room;
            room = room.saturating_sub(size);
            fits
        })
        .collect();
    kept.reverse();
    let dropped = dropped.saturating_add((total - kept.len()) as u64);
    Response::Tab {
        reply: Reply::Logs {
            entries: kept,
            dropped,
        },
    }
}

/// `text` with control characters other than line breaks and tabs shown
/// as U+FFFD, so a page cannot send escapes to the caller's terminal.
fn printable(text: &str) -> String {
    text.chars()
        .map(|c| {
            if c.is_control() && c != '\n' && c != '\t' {
                '\u{fffd}'
            } else {
                c
            }
        })
        .collect()
}

#[cfg(test)]
mod tests;
