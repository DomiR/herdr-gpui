//! Back, and Cmd-B (Toggle Sidebar), right after a Linear issue's worktree
//! opened from the Linear section, return to that section and the issue;
//! otherwise Cmd-B toggles the sidebar as always.
use super::{
    sections::{local, show},
    *,
};
use crate::{controls::Command, endpoint::Step, orchestrator::OrchestratorView};
use gpui::{Entity, VisualTestContext};

const ISSUE: &str = "linear:linear.app:acme:ENG-12";

fn panel(view: &Entity<HerdrWindow>, cx: &mut VisualTestContext) -> ActivityPanel {
    cx.update(|_, cx| view.read(cx).activity.panel)
}

fn section(view: &Entity<HerdrWindow>, cx: &mut VisualTestContext) -> Entity<OrchestratorView> {
    cx.update(|_, cx| {
        let view = view.read(cx);
        let id = view.activity.section_view.unwrap();
        view.orchestrators[&id].view().clone()
    })
}

/// The Linear section opens issue `ISSUE`'s run, leaving for Workspaces,
/// as Go to worktree does.
fn leave_for_worktree(view: &Entity<HerdrWindow>, cx: &mut VisualTestContext) {
    show(view, cx, ActivityPanel::Linear);
    let section = section(view, cx);
    cx.update(|_, cx| {
        section.update(cx, |section, _| {
            section.set_worktree_issue(Some(ISSUE.into()))
        });
        view.update(cx, |view, cx| {
            let id = view.activity.section_view.unwrap();
            view.leave_activity_section(id, cx);
        })
    });
    assert_eq!(panel(view, cx), ActivityPanel::Workspaces);
}

fn run(view: &Entity<HerdrWindow>, command: Command, cx: &mut VisualTestContext) {
    cx.update(|window, cx| view.update(cx, |view, cx| view.command(command, window, cx)));
}

fn sidebar_visible(view: &Entity<HerdrWindow>, cx: &mut VisualTestContext) -> bool {
    cx.update(|_, cx| view.read(cx).sidebar_visible)
}

#[gpui::test]
fn toggle_sidebar_returns_to_the_linear_issue_once(cx: &mut gpui::TestAppContext) {
    let (view, cx) = local(cx);
    leave_for_worktree(&view, cx);
    let visible = sidebar_visible(&view, cx);
    run(&view, Command::ToggleSidebar, cx);
    assert_eq!(panel(&view, cx), ActivityPanel::Linear);
    assert_eq!(sidebar_visible(&view, cx), visible);
    // The way back is used up: the issue is forgotten, and in the section
    // the shortcut toggles the sidebar again.
    let section = section(&view, cx);
    assert!(cx.update(|_, cx| section.read(cx).worktree_issue().is_none()));
    run(&view, Command::ToggleSidebar, cx);
    assert_eq!(panel(&view, cx), ActivityPanel::Linear);
    assert_ne!(sidebar_visible(&view, cx), visible);
}

#[gpui::test]
fn back_returns_to_the_linear_issue(cx: &mut gpui::TestAppContext) {
    let (view, cx) = local(cx);
    leave_for_worktree(&view, cx);
    cx.update(|window, cx| view.update(cx, |view, cx| view.travel(Step::Back, window, cx)));
    assert_eq!(panel(&view, cx), ActivityPanel::Linear);
    let section = section(&view, cx);
    assert!(cx.update(|_, cx| section.read(cx).worktree_issue().is_none()));
}

#[gpui::test]
fn picking_a_section_gives_toggle_sidebar_back(cx: &mut gpui::TestAppContext) {
    let (view, cx) = local(cx);
    leave_for_worktree(&view, cx);
    show(&view, cx, ActivityPanel::Workspaces);
    let visible = sidebar_visible(&view, cx);
    run(&view, Command::ToggleSidebar, cx);
    assert_eq!(panel(&view, cx), ActivityPanel::Workspaces);
    assert_ne!(sidebar_visible(&view, cx), visible);
}

#[gpui::test]
fn leaving_another_section_leaves_toggle_sidebar_alone(cx: &mut gpui::TestAppContext) {
    let (view, cx) = local(cx);
    show(&view, cx, ActivityPanel::Issues);
    let section = section(&view, cx);
    cx.update(|_, cx| {
        // Even a remembered Linear issue: Issues is not where it came from.
        section.update(cx, |section, _| {
            section.set_worktree_issue(Some(ISSUE.into()))
        });
        view.update(cx, |view, cx| {
            let id = view.activity.section_view.unwrap();
            view.leave_activity_section(id, cx);
        })
    });
    let visible = sidebar_visible(&view, cx);
    run(&view, Command::ToggleSidebar, cx);
    assert_eq!(panel(&view, cx), ActivityPanel::Workspaces);
    assert_ne!(sidebar_visible(&view, cx), visible);
}
