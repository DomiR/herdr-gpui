//! App-kept closes use the same preflight and consent as Herdr 1, without
//! needing a Herdr 1 snapshot or connection.
use super::*;
use crate::{
    browser::Location,
    core_terminals::panes::Panes,
    menu::Page,
    sidebar::layout_tests::{SidebarFixture, fixture_window},
};
use gpui::{AppContext, Entity, TestAppContext, VisualTestContext};
use sheprd_backend::TerminalId;
use sheprd_vcs::git::Identity;
use std::path::Path;

fn window<'a>(
    cx: &'a mut TestAppContext,
    folder: &Path,
) -> (Entity<SheprdWindow>, &'a mut VisualTestContext, String) {
    window_with_identity(cx, folder, None)
}

fn window_with_identity<'a>(
    cx: &'a mut TestAppContext,
    folder: &Path,
    identity: Option<Identity>,
) -> (Entity<SheprdWindow>, &'a mut VisualTestContext, String) {
    let (fixture, cx) = cx.add_window_view(|window, cx| {
        let view = cx.new(|cx| fixture_window(window, cx));
        SidebarFixture(view)
    });
    let view = cx.update(|_, cx| fixture.read(cx).0.clone());
    let id = cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.live.snapshot = None;
            view.live.missing_installation = true;
            view.config.confirm_close_workspace = true;
            view.core_terminals
                .seed_checkout_for_test(folder.to_str().unwrap(), identity);
            let location =
                Location::core_terminal(Panes::single(TerminalId::new("t1").unwrap()), false);
            let id = view
                .open_kept_workspace(None, folder, location, window, cx)
                .unwrap();
            view.open_workspace_dialog_for(
                &id,
                WorkspaceAction::Close,
                Default::default(),
                window,
                cx,
            );
            id
        })
    });
    (view, cx, id)
}

fn git(folder: &Path, hooks: &Path, args: &[&str]) -> String {
    let result = std::process::Command::new("git")
        .arg("-C")
        .arg(folder)
        .args([
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.invalid",
            "-c",
            "commit.gpgsign=false",
            "-c",
        ])
        .arg(format!("core.hooksPath={}", hooks.display()))
        .args(args)
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_INDEX_FILE")
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    String::from_utf8(result.stdout).unwrap().trim().to_owned()
}

/// A published checkout, with metadata read from that checkout rather than
/// invented paths. Remote tests deliberately report this same local path.
fn clean_repository() -> (tempfile::TempDir, Identity) {
    let folder = tempfile::tempdir().unwrap();
    let hooks = tempfile::tempdir().unwrap();
    git(folder.path(), hooks.path(), &["init", "-b", "test"]);
    git(
        folder.path(),
        hooks.path(),
        &["commit", "--allow-empty", "-m", "initial"],
    );
    git(
        folder.path(),
        hooks.path(),
        &["update-ref", "refs/remotes/origin/test", "HEAD"],
    );
    let identity = Identity {
        branch: Some(git(
            folder.path(),
            hooks.path(),
            &["symbolic-ref", "--short", "HEAD"],
        )),
        key: git(
            folder.path(),
            hooks.path(),
            &["rev-parse", "--path-format=absolute", "--git-common-dir"],
        ),
        label: "test".into(),
        linked: false,
    };
    (folder, identity)
}

#[gpui::test]
fn kept_close_dialog_local_checkout_is_known_clean(cx: &mut TestAppContext) {
    let (folder, identity) = clean_repository();
    let (view, cx, id) = window_with_identity(cx, folder.path(), Some(identity));
    cx.run_until_parked();
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            assert!(!view.selected_is_remote());
            assert!(view.live.snapshot.is_none());
            view.update_workspace_dialog(window, cx);
            let check = view.menu.close_check.as_ref().unwrap();
            let report = check.report.as_ref().unwrap();
            assert!(
                !report.unknown,
                "the real local dialog must run the preflight"
            );
            assert!(!report.dirty && !report.unpushed);
            assert!(!report.needs_consent());
            assert!(check.allows("", true));
            assert!(view.menu.input.is_none());
            assert!(view.is_kept_workspace(&id));
        });
    });
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[gpui::test]
fn kept_close_dialog_remote_checkout_is_unknown_without_a_local_probe(cx: &mut TestAppContext) {
    let (folder, identity) = clean_repository();
    let (view, cx, _) = window_with_identity(cx, folder.path(), Some(identity.clone()));
    let id = cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.dismiss_menu(window, cx);
            let mut remote = crate::endpoint::Endpoint::new(
                "ssh:box".into(),
                "Box".into(),
                sheprd_client::ConnectTarget::Ssh {
                    target: "nobody@box.invalid".into(),
                    session: "default".into(),
                },
                true,
            );
            remote.live.snapshot = None;
            let scope = crate::browser::scope(&remote);
            // Never connect: every possible core request gets the fixture's
            // missing-backend answer, as in the kept-dispatch tests.
            view.core_terminals.set_remote_hub_for_test(
                scope.clone(),
                remote.connection.target.clone(),
                crate::core_terminals::Hub::with_join(|_| {
                    Err(crate::core_terminals::Error::NotBundled)
                }),
            );
            view.core_terminals.seed_remote_checkout_for_test(
                &scope,
                folder.path().to_str().unwrap(),
                Some(identity),
            );
            view.endpoints.push(remote);
            view.endpoints[0].live = std::mem::take(&mut view.live);
            view.selected_endpoint = 1;
            view.live = std::mem::take(&mut view.endpoints[1].live);
            let location =
                Location::core_terminal(Panes::single(TerminalId::new("t2").unwrap()), false);
            let id = view
                .open_kept_workspace(None, folder.path(), location, window, cx)
                .unwrap();
            view.open_workspace_dialog_for(
                &id,
                WorkspaceAction::Close,
                Default::default(),
                window,
                cx,
            );
            assert!(view.selected_is_remote());
            assert_eq!(
                view.menu.target.as_ref().unwrap().branch.as_deref(),
                Some("test")
            );
            id
        })
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.update_workspace_dialog(window, cx);
            let check = view.menu.close_check.as_ref().unwrap();
            // This path is a known-clean local canary: probing it would give
            // a clean report and fail this assertion, not a coincidental error.
            assert!(check.report.as_ref().unwrap().unknown);
            assert!(!check.allows("", true));
            assert!(check.allows("close", true));
            assert!(view.menu.input.is_some());
            view.submit_workspace_dialog(window, cx);
            assert!(view.is_kept_workspace(&id));
        });
    });
}

fn real_repository() -> tempfile::TempDir {
    let folder = tempfile::tempdir().unwrap();
    let hooks = tempfile::tempdir().unwrap();
    git(folder.path(), hooks.path(), &["init", "-b", "test"]);
    git(
        folder.path(),
        hooks.path(),
        &["commit", "--allow-empty", "-m", "initial"],
    );
    git(
        folder.path(),
        hooks.path(),
        &["update-ref", "refs/remotes/origin/test", "HEAD"],
    );
    folder
}

#[gpui::test]
fn kept_close_check_does_not_depend_on_the_identity_cache(cx: &mut TestAppContext) {
    // The cache has no branch for this folder, as right after it opens.
    let folder = real_repository();
    let (view, cx, _) = window(cx, folder.path());
    cx.run_until_parked();
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            assert!(view.menu.target.as_ref().unwrap().branch.is_none());
            view.update_workspace_dialog(window, cx);
            let report = view
                .menu
                .close_check
                .as_ref()
                .unwrap()
                .report
                .as_ref()
                .unwrap();
            assert!(!report.unknown, "the worker reads the folder itself");
            assert!(!report.dirty && !report.unpushed);
        });
    });
}

#[gpui::test]
fn kept_close_refuses_a_branch_switched_after_the_check(cx: &mut TestAppContext) {
    let folder = real_repository();
    let hooks = tempfile::tempdir().unwrap();
    let (view, cx, id) = window(cx, folder.path());
    cx.run_until_parked();
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.update_workspace_dialog(window, cx);
            assert!(view.menu.close_check.as_ref().unwrap().report.is_some());
        });
    });
    // The session cache is slow to notice; the folder is switched on disk.
    git(
        folder.path(),
        hooks.path(),
        &["checkout", "-q", "-b", "other"],
    );
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.submit_workspace_dialog(window, cx);
            assert!(view.is_kept_workspace(&id), "a stale report cannot close");
            assert!(view.menu.error.is_some());
            assert!(
                view.menu.close_check.as_ref().unwrap().report.is_none(),
                "the check starts again for the new branch"
            );
        });
    });
}

#[gpui::test]
fn kept_close_accepts_the_cache_learning_an_unchanged_checkout(cx: &mut TestAppContext) {
    let folder = real_repository();
    let hooks = tempfile::tempdir().unwrap();
    let (view, cx, id) = window(cx, folder.path());
    cx.run_until_parked();
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.update_workspace_dialog(window, cx);
            let key = git(
                folder.path(),
                hooks.path(),
                &["rev-parse", "--path-format=absolute", "--git-common-dir"],
            );
            view.core_terminals.seed_checkout_for_test(
                folder.path().to_str().unwrap(),
                Some(Identity {
                    branch: Some("test".into()),
                    key,
                    label: "test".into(),
                    linked: false,
                }),
            );
            view.refresh_core_session(cx);
            view.submit_workspace_dialog(window, cx);
            assert!(
                !view.is_kept_workspace(&id),
                "an unchanged clean checkout closes"
            );
        });
    });
}

#[gpui::test]
fn kept_close_finishes_without_a_herdr_snapshot_and_requires_unknown_consent(
    cx: &mut TestAppContext,
) {
    let folder = tempfile::tempdir().unwrap();
    let (view, cx, id) = window(cx, folder.path());
    view.read_with(cx, |view, _| {
        assert!(view.live.snapshot.is_none());
        assert!(
            view.menu.close_check.is_some(),
            "the real dialog must start a check"
        );
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.update_workspace_dialog(window, cx);
            let check = view.menu.close_check.as_ref().unwrap();
            assert!(check.report.as_ref().unwrap().unknown);
            assert!(!check.allows("", true));
            assert!(check.status(true).contains("could not be verified"));
            assert!(view.menu.input.is_some());
            view.submit_workspace_dialog(window, cx);
            assert!(view.is_kept_workspace(&id));
        });
        window.draw(cx).clear(cx);
    });
    assert!(cx.debug_bounds("close-git-status").is_some());
    cx.simulate_keystrokes("enter");
    view.read_with(cx, |view, _| assert!(view.is_kept_workspace(&id)));
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.menu.input = Some(DialogInput::new("close".into()));
            view.submit_workspace_dialog(window, cx);
            assert!(!view.is_kept_workspace(&id));
            assert!(view.menu.page.is_none());
        })
    });
}

#[gpui::test]
fn kept_close_programmatic_submission_cannot_bypass_pending_or_missing_check(
    cx: &mut TestAppContext,
) {
    let folder = tempfile::tempdir().unwrap();
    let (view, cx, id) = window(cx, folder.path());
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.menu.input = Some(DialogInput::new("close".into()));
            view.submit_workspace_dialog(window, cx);
            assert!(
                view.is_kept_workspace(&id),
                "pending check blocks even typed consent"
            );
            view.menu.close_check = None;
            view.submit_workspace_dialog(window, cx);
            assert!(
                view.is_kept_workspace(&id),
                "missing check cannot authorize close"
            );
            assert_eq!(view.menu.page, Some(Page::Dialog(WorkspaceAction::Close)));
        })
    });
}

#[gpui::test]
fn kept_close_unconfirmed_unknown_finishes_automatically(cx: &mut TestAppContext) {
    let folder = tempfile::tempdir().unwrap();
    let (view, cx, id) = window(cx, folder.path());
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.config.confirm_close_workspace = false;
            view.submit_workspace_dialog(window, cx);
            assert!(
                view.is_kept_workspace(&id),
                "even unconfirmed closes wait for the check"
            );
        })
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.update_workspace_dialog(window, cx);
            assert!(!view.is_kept_workspace(&id));
        })
    });
}

#[gpui::test]
fn kept_close_revalidates_host_and_checkout_before_submission(cx: &mut TestAppContext) {
    let folder = tempfile::tempdir().unwrap();
    let (view, cx, id) = window(cx, folder.path());
    cx.run_until_parked();
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.update_workspace_dialog(window, cx);
            view.menu.input = Some(DialogInput::new("close".into()));
            let endpoint = view.menu.endpoint_target;
            view.menu.endpoint_target.0 = view.selection_epoch.wrapping_add(1);
            view.submit_workspace_dialog(window, cx);
            assert!(view.is_kept_workspace(&id));
            assert!(view.menu.error.is_some());
            view.menu.endpoint_target = endpoint;
            // The session learns a new checkout identity while the old
            // dialog and its completed report stay open.
            view.core_terminals.seed_checkout_for_test(
                folder.path().to_str().unwrap(),
                Some(Identity {
                    branch: Some("replacement".into()),
                    key: folder.path().join(".git").to_str().unwrap().into(),
                    label: "replacement".into(),
                    linked: false,
                }),
            );
            view.refresh_core_session(cx);
            view.menu.error = None;
            view.submit_workspace_dialog(window, cx);
            assert!(view.is_kept_workspace(&id));
            assert!(view.menu.error.is_some());
        })
    });
}

#[gpui::test]
fn kept_close_empty_workspace_preserves_another_workspaces_terminal_tab(cx: &mut TestAppContext) {
    use crate::browser::Store;
    let folder = tempfile::tempdir().unwrap();
    let (view, cx, protected) = window(cx, folder.path());
    let (empty, tabs) = cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.dismiss_menu(window, cx);
            let scope = crate::browser::scope(&view.endpoints[0]);
            let tabs: Vec<_> = cx
                .global::<Store>()
                .in_workspace(&scope, &protected)
                .cloned()
                .collect();
            assert!(matches!(
                tabs[0].location,
                Some(Location::CoreTerminal { .. })
            ));
            let empty =
                Store::update(cx, |store| store.keep_workspace(&scope, "empty", "", true)).unwrap();
            assert!(
                cx.global::<Store>()
                    .in_workspace(&scope, &empty)
                    .next()
                    .is_none()
            );
            view.refresh_core_session(cx);
            view.open_workspace_dialog_for(
                &empty,
                WorkspaceAction::Close,
                Default::default(),
                window,
                cx,
            );
            (empty, tabs)
        })
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.update_workspace_dialog(window, cx);
            assert!(
                view.menu
                    .close_check
                    .as_ref()
                    .unwrap()
                    .report
                    .as_ref()
                    .unwrap()
                    .unknown
            );
            view.menu.input = Some(DialogInput::new("close".into()));
            view.submit_workspace_dialog(window, cx);
            assert!(!view.is_kept_workspace(&empty));
            assert!(view.is_kept_workspace(&protected));
            let scope = crate::browser::scope(&view.endpoints[0]);
            let remaining: Vec<_> = cx
                .global::<Store>()
                .in_workspace(&scope, &protected)
                .cloned()
                .collect();
            assert_eq!(
                remaining, tabs,
                "the other workspace keeps its terminal and panes"
            );
        })
    });
}

#[gpui::test]
fn kept_close_checks_local_git_and_never_probes_remote_paths(cx: &mut TestAppContext) {
    use crate::{menu::workspace::Source, menu::workspace_close::CloseCheck};
    let folder = tempfile::tempdir().unwrap();
    let hooks = tempfile::tempdir().unwrap();
    let git = |args: &[&str]| git(folder.path(), hooks.path(), args);
    git(&["init", "-b", "test"]);
    git(&["commit", "--allow-empty", "-m", "initial"]);
    git(&["update-ref", "refs/remotes/origin/test", "HEAD"]);
    let (view, cx, id) = window(cx, folder.path());
    // Exercise directory fallback as well as metadata keys. Both go through
    // the real worker's branch/repository verification, not a canned report.
    for metadata in [false, true] {
        for (local, dirty, unpushed) in [
            (true, false, false),
            (false, false, false),
            (true, true, false),
            (true, false, true),
        ] {
            if unpushed {
                git(&["update-ref", "-d", "refs/remotes/origin/test"]);
            }
            if dirty {
                std::fs::write(folder.path().join("new"), "untracked\n").unwrap();
            }
            cx.update(|window, cx| {
                view.update(cx, |view, cx| {
                    view.dismiss_menu(window, cx);
                    view.open_workspace_menu(&id, Default::default(), window, cx);
                    let target = view.menu.target.as_mut().unwrap();
                    target.source = Source::Directory(folder.path().to_str().unwrap().into());
                    target.branch = Some("test".into());
                    target.worktree =
                        metadata.then(|| sheprd_client::protocol::ClientShellWorktree {
                            key: folder.path().join(".git").to_str().unwrap().into(),
                            label: "test".into(),
                            is_linked_worktree: false,
                        });
                    view.menu.close_check = Some(CloseCheck::start_kept(target, local, cx));
                })
            });
            cx.run_until_parked();
            cx.update(|_, cx| {
                view.update(cx, |view, _| {
                    let check = view.menu.close_check.as_mut().unwrap();
                    assert!(check.poll());
                    let report = check.report.as_ref().unwrap();
                    assert_eq!(report.unknown, !local);
                    assert_eq!(report.dirty, local && dirty);
                    assert_eq!(report.unpushed, local && unpushed);
                    assert_eq!(check.allows("", true), local && !dirty && !unpushed);
                    assert!(check.allows("close", true));
                })
            });
            if dirty {
                std::fs::remove_file(folder.path().join("new")).unwrap();
            }
            if unpushed {
                git(&["update-ref", "refs/remotes/origin/test", "HEAD"]);
            }
        }
    }
    // The worker must not accept a relative folder or one outside Git, and it
    // reads the folder itself: stale cache metadata changes nothing.
    let checkout = folder.path().to_str().unwrap();
    let nonrepo = hooks.path().to_str().unwrap();
    for (cwd, branch, key, known) in [
        ("relative", Some("test"), None, false),
        (nonrepo, Some("test"), None, false),
        (checkout, None, None, true),
        (checkout, Some("test\n"), None, true),
        (checkout, Some("replacement"), None, true),
        (checkout, Some("test"), Some(nonrepo), true),
        (checkout, Some("test"), Some("relative/.git"), true),
    ] {
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.dismiss_menu(window, cx);
                view.open_workspace_menu(&id, Default::default(), window, cx);
                let target = view.menu.target.as_mut().unwrap();
                target.source = Source::Directory(cwd.into());
                target.branch = branch.map(str::to_owned);
                target.worktree = key.map(|key| sheprd_client::protocol::ClientShellWorktree {
                    key: key.into(),
                    label: "test".into(),
                    is_linked_worktree: false,
                });
                view.menu.close_check = Some(CloseCheck::start_kept(target, true, cx));
            });
        });
        cx.run_until_parked();
        cx.update(|_, cx| {
            view.update(cx, |view, _| {
                let check = view.menu.close_check.as_mut().unwrap();
                assert!(check.poll());
                let report = check.report.as_ref().unwrap();
                assert_eq!(report.unknown, !known);
                assert_eq!(check.allows("", true), known);
                assert!(check.allows("close", true));
            });
        });
    }
}
