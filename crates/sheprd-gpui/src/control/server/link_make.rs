use super::{target, window_for};
use crate::{
    SheprdWindow, NavigationTarget,
    control::protocol::{
        Call, ErrorCode, Response,
        link::{LinkMakeOp, LinkTarget},
    },
    link::focus::build::link_for,
};
use gpui::App;

pub(super) fn serve(call: Call<LinkMakeOp>, cx: &mut App) -> Response {
    if !call.op.bounded() {
        return Response::error(ErrorCode::InvalidRequest, "The pane ID is invalid");
    }
    let handle = match window_for(&call.caller, cx) {
        Ok(handle) => handle,
        Err(response) => return response,
    };
    handle
        .update(cx, |view, _, _| make(view, &call.caller, call.op))
        .unwrap_or_else(|_| Response::error(ErrorCode::NoWindow, "The window closed"))
}

fn make(view: &SheprdWindow, caller: &crate::control::protocol::Caller, op: LinkMakeOp) -> Response {
    let Some(pane_id) = op.pane_id.or_else(|| caller.pane_id.clone()) else {
        return Response::error(
            ErrorCode::InvalidRequest,
            "Not running in a Herdr pane; pass --pane ID",
        );
    };
    let target = target(caller);
    let Some((index, workspace_id)) = target
        .strictness()
        .iter()
        .find_map(|strict| view.control_target(&target, *strict))
    else {
        return Response::error(
            ErrorCode::WorkspaceNotFound,
            "The caller's Herdr workspace is not shown",
        );
    };
    let endpoint = &view.endpoints[index];
    let live = if index == view.selected_endpoint {
        &view.live
    } else {
        &endpoint.live
    };
    if !live.status.is_connected() {
        return Response::error(ErrorCode::Busy, "The caller's Herdr host is not connected");
    }
    let Some(snapshot) = live.snapshot.as_deref() else {
        return Response::error(ErrorCode::NotFound, "The caller's Herdr state is not ready");
    };
    let Some(pane) = snapshot
        .panes
        .iter()
        .find(|pane| pane.pane_id == pane_id && pane.workspace_id == workspace_id)
    else {
        return Response::error(
            ErrorCode::NotFound,
            format!("No pane named {pane_id} is in the caller's workspace"),
        );
    };
    let navigation = match op.target {
        LinkTarget::Agent => NavigationTarget::Pane(pane.pane_id.as_str()),
        LinkTarget::Tab => NavigationTarget::Tab(pane.tab_id.as_str()),
        LinkTarget::Workspace => NavigationTarget::Workspace(pane.workspace_id.as_str()),
    };
    let Some(made) = link_for(snapshot, navigation, Some(endpoint.id.as_str())) else {
        return Response::error(
            ErrorCode::NotFound,
            match op.target {
                LinkTarget::Agent => String::from("The pane has no named agent"),
                LinkTarget::Tab => String::from("The caller's Herdr tab is not linkable"),
                LinkTarget::Workspace => {
                    String::from("The caller's Herdr workspace is not linkable")
                }
            },
        );
    };
    Response::Link {
        reply: crate::control::protocol::link::Reply {
            url: made.url(),
            markdown: made.markdown(),
            label: made.label,
        },
    }
}
