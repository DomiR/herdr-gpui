//! `link`: a `herdr-gpui://` link the shell started a second copy of the app
//! with, from a jump list entry, a notification, or another application.
//! It is parsed again here, since any local process can send one, and it
//! only ever shows something: a checkout, a device, a focus target, or a
//! notification's pane.

use super::{main_windows, not_found, window_for};
use crate::{
    control::protocol::{Call, ErrorCode, LinkOp, Response},
    link::Link,
};
use gpui::App;

pub(super) fn serve(call: Call<LinkOp>, cx: &mut App) -> Response {
    let link = match Link::parse(&call.op.url) {
        Ok(link) => link,
        Err(error) => {
            return Response::error(ErrorCode::InvalidRequest, format!("Invalid link: {error}"));
        }
    };
    if let Link::Notification { tag } = &link {
        // The notification knows its own window.
        return if crate::window::system_notifications::open(tag, cx) {
            Response::done("Opened the notification's pane")
        } else {
            not_found("That notification is no longer known")
        };
    }
    // Only a checkout needs a local-connected window chosen across windows.
    // Device and focus links use the caller's window, like `--window` does.
    let connected = (matches!(link, Link::Open { .. }) && call.caller.window.is_none())
        .then(|| {
            main_windows(cx).into_iter().find(|handle| {
                handle
                    .read_with(cx, |view, _| crate::link::follow::shows_local(view))
                    .unwrap_or(false)
            })
        })
        .flatten();
    let handle = match connected.map_or_else(|| window_for(&call.caller, cx), Ok) {
        Ok(handle) => handle,
        Err(response) => return response,
    };
    crate::link::follow::follow(link, handle, cx)
}

#[cfg(test)]
mod tests;
