use super::{Context, Output, send};
use crate::cli::control::LinkCommand;
use crate::control::protocol::{Call, Request, Response, link::LinkMakeOp};

pub(super) fn link(command: LinkCommand, context: &Context) -> crate::Result<Output> {
    let mut caller = context.caller()?;
    let Some(pane_id) = command.pane.or_else(|| caller.pane_id.clone()) else {
        return Ok(Output::usage("Not running in a Herdr pane; pass --pane ID"));
    };
    caller.pane_id = Some(pane_id.clone());
    let response = send(&Request::LinkMake(Call {
        caller,
        op: LinkMakeOp {
            pane_id: Some(pane_id),
            target: command.target,
        },
    }))?;
    match response {
        Response::Link { reply } => Output::data(reply.markdown.clone(), &reply),
        response => Err(super::unexpected(response)),
    }
}
