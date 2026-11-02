//! Go to worktree on a Linear issue's card and row, shown while the pointer
//! is on it: the issue's newest run opens as its Open button does
//! (`OrchestratorView::open_run`). Opening a Linear issue's run remembers
//! the issue, so Back, or Cmd-B right after, brings the user back to its
//! page (`orchestrator::section`).

use super::{OrchestratorView, rows::ItemRow};
use crate::i18n::fl;
use crate::orchestrator::{Backend, Provider};
use gpui::{prelude::*, *};

/// The group a card's button shows on hover with.
pub(super) const CARD_GROUP: &str = "orchestrator-card";
/// The group a row's button shows on hover with.
pub(super) const ROW_GROUP: &str = "orchestrator-row";

impl OrchestratorView {
    /// The newest run of `row`'s item, when the item is a Linear issue and
    /// the run has a Herdr workspace to show.
    fn worktree_run(&self, row: &ItemRow) -> Option<usize> {
        let run = row.run?.run;
        let snapshot = &self.repos.get(row.repo)?.snapshot;
        if snapshot.items.get(row.item)?.key.source.provider != Provider::Linear {
            return None;
        }
        let workspace = snapshot.runs.get(run)?.workspace.as_ref()?;
        (workspace.backend == Backend::Herdr && !workspace.id.is_empty()).then_some(run)
    }

    /// Go to worktree for `row`, hidden until the pointer is on the element
    /// of `group`, when its issue has a worktree to go to.
    pub(super) fn worktree_button(
        &self,
        row: &ItemRow,
        group: &'static str,
        cx: &mut Context<Self>,
    ) -> Option<Stateful<Div>> {
        let run = self.worktree_run(row)?;
        let key = self.row_item(row).key.canonical();
        let selector = format!("orchestrator-worktree-{key}");
        Some(
            self.look
                .compact_button(
                    SharedString::from(selector.clone()),
                    fl!("orchestrator-linear-go-to-worktree"),
                )
                .debug_selector(move || selector.clone())
                .invisible()
                .group_hover(group, |button| button.visible())
                // The card or row under it neither opens nor starts a drag.
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.go_to_worktree(key.clone(), run, cx);
                })),
        )
    }

    /// Opens `key`'s page, so the way back finds it, then shows its run
    /// `run`.
    fn go_to_worktree(&mut self, key: String, run: usize, cx: &mut Context<Self>) {
        self.selected = Some(key.clone());
        self.open_detail(key, cx);
        self.open_run(run, cx);
    }

    /// Remembers the Linear issue `run` works on as the one to return to,
    /// or forgets it for a run of anything else.
    pub(super) fn leave_for_run(&mut self, run: usize) {
        self.worktree_issue = self
            .snapshot()
            .runs
            .get(run)
            .map(|run| run.item_key.as_str())
            .filter(|key| {
                self.item(key)
                    .is_some_and(|item| item.key.source.provider == Provider::Linear)
            })
            .map(str::to_owned);
    }

    /// The Linear issue whose run was last opened, while the window may
    /// still bring the user back to it.
    pub(crate) fn worktree_issue(&self) -> Option<&str> {
        self.worktree_issue.as_deref()
    }

    /// Forgets the issue to return to, as leaving for anything but its run
    /// does.
    pub(crate) fn forget_worktree_issue(&mut self) {
        self.worktree_issue = None;
    }

    /// Opens the page of the issue to return to, and forgets it.
    pub(crate) fn return_to_worktree_issue(&mut self, cx: &mut Context<Self>) {
        let Some(key) = self.worktree_issue.take() else {
            return;
        };
        if self.item(&key).is_none() {
            return;
        }
        self.selected = Some(key.clone());
        self.open_detail(key, cx);
    }

    /// Sets the issue to return to, for tests outside the view.
    #[cfg(test)]
    pub(crate) fn set_worktree_issue(&mut self, key: Option<String>) {
        self.worktree_issue = key;
    }
}
