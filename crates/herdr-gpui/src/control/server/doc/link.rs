use super::super::socket;
use crate::control::protocol::{Response, doc};
use gpui::App;
use std::path::Path;

pub(super) fn serve(
    path: &Path,
    display: String,
    off: bool,
    incoming: socket::Incoming,
    cx: &mut App,
) {
    let Some(handle) = crate::doc_link::links(cx) else {
        return incoming.respond(Response::Doc {
            reply: doc::Reply::Link {
                path: display,
                status: off_status(),
            },
        });
    };
    if off {
        handle.update(cx, |links, cx| links.disable(path, cx));
    }
    let status = handle
        .read(cx)
        .status(path)
        .map(wire_status)
        .unwrap_or_else(off_status);
    incoming.respond(Response::Doc {
        reply: doc::Reply::Link {
            path: display,
            status,
        },
    });
}

fn off_status() -> doc::LinkStatus {
    doc::LinkStatus {
        acked: None,
        agent: None,
        mode: doc::LinkMode::Off,
        pane: None,
        pending: None,
    }
}

fn wire_status(status: crate::doc_link::LinkStatus) -> doc::LinkStatus {
    doc::LinkStatus {
        acked: status.acked.map(|version| version.0),
        agent: status.agent_name,
        mode: wire_mode(status.mode),
        pane: status.pane_id,
        pending: status.pending.map(|pending| doc::LinkPending {
            from: pending.from.0,
            in_flight: pending.in_flight,
            to: pending.to.0,
        }),
    }
}

fn wire_mode(mode: crate::doc_link::Mode) -> doc::LinkMode {
    match mode {
        crate::doc_link::Mode::Off => doc::LinkMode::Off,
        crate::doc_link::Mode::On => doc::LinkMode::On,
        crate::doc_link::Mode::Paused => doc::LinkMode::Paused,
        crate::doc_link::Mode::Restoring => doc::LinkMode::Restoring,
        crate::doc_link::Mode::Starting => doc::LinkMode::Starting,
    }
}
