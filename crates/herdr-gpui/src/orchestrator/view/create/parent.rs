//! A new issue made under another, as Linear's own Add sub-issue does: the
//! dialog names the parent in its header, starts in the parent's team, and
//! creates the issue with the parent's id. Create more keeps the parent, so
//! several sub-issues are written in a row.

use super::super::OrchestratorView;
use crate::{i18n::fl, orchestrator::Item};
use gpui::{prelude::*, *};

/// The issue a new one is created under.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::orchestrator::view) struct Parent {
    /// Linear's id for it, which `parentId` takes.
    pub(in crate::orchestrator::view) id: String,
    pub(in crate::orchestrator::view) identifier: String,
    pub(in crate::orchestrator::view) title: String,
}

impl Parent {
    /// `item` as a parent, when Linear has given it an id.
    pub(in crate::orchestrator::view) fn of(item: &Item) -> Option<Self> {
        let id = item.tracker.as_ref()?.id.clone()?;
        Some(Self {
            id,
            identifier: item.identifier.clone(),
            title: item.title.clone(),
        })
    }

    /// The key of the team its identifier starts with, such as `ENG` for
    /// `ENG-12`, which the sub-issue starts in.
    pub(super) fn team_key(&self) -> &str {
        self.identifier
            .rsplit_once('-')
            .map_or(self.identifier.as_str(), |(key, _)| key)
    }
}

impl OrchestratorView {
    /// Opens the new issue dialog for a sub-issue of `item`. A dialog
    /// already open moves under `item`, keeping what was written, and back
    /// to its team.
    pub(in crate::orchestrator::view) fn open_linear_sub_issue(
        &mut self,
        item: &Item,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(parent) = Parent::of(item) else {
            return;
        };
        self.open_linear_create(None, window, cx);
        if let Some(dialog) = &mut self.create.dialog
            && dialog.parent.as_ref() != Some(&parent)
        {
            dialog.parent = Some(parent);
            dialog.team = None;
            dialog.asked_state = None;
        }
        self.apply_options();
        cx.notify();
    }

    /// What the dialog's header says it makes: a new issue, or a sub-issue
    /// of its parent with a button that makes it a top-level issue again.
    pub(super) fn render_create_kind(&self, cx: &mut Context<Self>) -> AnyElement {
        let look = &self.look;
        let Some(parent) = self
            .create
            .dialog
            .as_ref()
            .and_then(|dialog| dialog.parent.as_ref())
        else {
            return look
                .muted(fl!("orchestrator-linear-new-issue"))
                .into_any_element();
        };
        div()
            .id("linear-create-parent")
            .debug_selector(|| "linear-create-parent".into())
            .min_w_0()
            .flex()
            .items_center()
            .gap_1()
            .child(look.muted(fl!("orchestrator-linear-sub-issue-of")))
            .child(look.mono(parent.identifier.clone()).flex_none())
            .child(
                div()
                    .min_w_0()
                    .truncate()
                    .text_color(rgb(look.theme.subtext()))
                    .child(parent.title.clone()),
            )
            .child(
                look.icon_button(
                    "linear-create-parent-clear",
                    "icons/close.svg",
                    fl!("orchestrator-linear-not-a-sub-issue"),
                )
                .on_click(cx.listener(|this, _, _, cx| {
                    if let Some(dialog) = &mut this.create.dialog {
                        dialog.parent = None;
                    }
                    cx.notify();
                })),
            )
            .into_any_element()
    }
}
