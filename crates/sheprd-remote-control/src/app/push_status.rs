//! `push-status`: how the daemon running on this computer sends iPhone
//! pushes, for Sheprd on another computer, which runs it over SSH to show
//! every linked host's push delivery beside its own.

use crate::control::{self, Request, Response, push::PushStatus};
use std::path::Path;

/// What the daemon whose state lives in `state_dir` answers, or `None`
/// when none runs or it is too old to know the request. Blocking.
pub fn read(state_dir: &Path) -> Option<PushStatus> {
    match control::request(&control::address(state_dir), &Request::PushStatus) {
        Ok(Response::PushStatus(status)) => Some(status),
        _ => None,
    }
}
