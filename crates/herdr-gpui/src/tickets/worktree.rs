//! Opening the worktree herdr-dog started a ticket in, from the ticket's
//! page: its folder's workspace is focused, or started there, as Open
//! Project does, and the window leaves the section for the terminals. Back
//! returns to the board. A worktree on another machine is not opened.

use crate::{HerdrWindow, sidebar::ActivityPanel};
use gpui::{Context, Window};
use std::path::PathBuf;

impl HerdrWindow {
    pub(super) fn open_ticket_worktree(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(path) = self
            .ticket_board
            .opened
            .as_ref()
            .and_then(|opened| opened.start.as_ref())
            .and_then(|start| start.local_worktree())
            .map(PathBuf::from)
        else {
            return;
        };
        if let Err(error) = self.open_project_at(path, window, cx) {
            self.ticket_board.notice = Some(format!("Cannot open the worktree: {error}").into());
            cx.notify();
            return;
        }
        if self.activity.pick(ActivityPanel::Workspaces) {
            self.save_chrome();
        }
        self.activity.return_to = Some(ActivityPanel::TicketsNative);
        cx.notify();
    }
}
