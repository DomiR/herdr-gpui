//! A review tab follows its workspace: when the daemon reports the worktree
//! on another branch, a tab that remembers no checkout of its own is saved
//! and titled by that branch and read again there, its scope and notes
//! kept. One that does follows its own HEAD instead. A workspace gone from
//! the snapshot leaves its tab as it was.
use super::{HerdrWindow, changes, line, note, the, window};
use crate::{
    browser::{Location, ReviewCheckout, Store, TabId, scope},
    pull_request::Input,
    review::diff::Scope,
};
use gpui::{Entity, VisualTestContext};
use herdr_client::protocol::ClientShellWorktree;
use std::sync::Arc;

/// Absolute on every platform: Windows needs a drive, Unix a root.
fn repo_key() -> String {
    std::env::temp_dir()
        .join("repo/.git")
        .to_str()
        .unwrap()
        .into()
}

/// Shows workspace `w0` as a Herdr worktree of the repository on `branch`,
/// on the window's own local daemon, and lets the Git chip follow it.
fn worktree_on(view: &Entity<HerdrWindow>, cx: &mut VisualTestContext, branch: &str) {
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            let mut shown = (**view.live.snapshot.as_ref().unwrap()).clone();
            let space = shown
                .workspaces
                .iter_mut()
                .find(|workspace| workspace.workspace_id == "w0")
                .unwrap();
            space.branch = Some(branch.into());
            space.worktree = Some(ClientShellWorktree {
                key: repo_key(),
                label: "repo".into(),
                is_linked_worktree: true,
            });
            view.live.snapshot = Some(Arc::new(shown));
            view.live.status = crate::state::ConnectionStatus::Connected;
            view.live.local_daemon_peer = true;
            view.update_git();
        })
    });
}

/// Saves `id`, and its review, as the review of `checkout`.
fn reviewing(view: &Entity<HerdrWindow>, cx: &mut VisualTestContext, id: TabId, checkout: Input) {
    cx.update(|_, cx| {
        let location = Location::Review {
            checkout: ReviewCheckout::try_from(&checkout).unwrap(),
        };
        Store::update(cx, |store| store.visited(id, Some(location), None));
        view.update(cx, |view, _| {
            view.reviews.get_mut(&id).unwrap().checkout = checkout
        });
    });
}

fn saved(cx: &mut VisualTestContext, id: TabId) -> (Option<Location>, String) {
    cx.update(|_, cx| {
        let tab = cx.global::<Store>().get(id).unwrap();
        (tab.location.clone(), tab.title.clone())
    })
}

#[gpui::test]
fn a_review_follows_its_worktree_onto_another_branch(cx: &mut gpui::TestAppContext) {
    let (view, cx) = window(cx, None);
    cx.update(|window, cx| view.update(cx, |view, cx| view.seed_review(changes(), window, cx)));
    note(&view, cx, line(3), "Implement this");
    let id = view.read_with(cx, |view, _| the(view));
    let feature = Input {
        checkout: None,
        repo_key: Some(repo_key()),
        branch: "feature".into(),
    };
    reviewing(&view, cx, id, feature);
    // Another workspace's review waits until that workspace is focused.
    let elsewhere = cx.update(|_, cx| {
        let tab_scope = scope(&view.read(cx).endpoints[0]);
        Store::update(cx, |store| {
            store.open(
                tab_scope,
                "w1",
                Some(Location::Review {
                    checkout: ReviewCheckout {
                        repo_key: Some(repo_key()),
                        branch: "feature".into(),
                        checkout: None,
                    },
                }),
                None,
            )
        })
        .unwrap()
    });
    let before = saved(cx, elsewhere);
    let request = view.update(cx, |view, _| {
        let review = view.reviews.get_mut(&id).unwrap();
        review.scope = Scope::Branch;
        review.request
    });

    worktree_on(&view, cx, "local/preview");
    cx.update(|_, cx| view.update(cx, |view, cx| view.poll_reviews(cx)));

    let moved = Input {
        checkout: None,
        repo_key: Some(repo_key()),
        branch: "local/preview".into(),
    };
    let (location, title) = saved(cx, id);
    assert_eq!(
        location,
        Some(Location::Review {
            checkout: ReviewCheckout::try_from(&moved).unwrap(),
        })
    );
    assert_eq!(title, "Review \u{00b7} local/preview");
    let request = view.read_with(cx, |view, _| {
        let review = &view.reviews[&id];
        assert_eq!(review.checkout, moved);
        assert_eq!(review.scope, Scope::Branch);
        assert_eq!(review.notes.len(), 1);
        assert!(review.request > request, "read again on the new branch");
        review.request
    });
    assert_eq!(saved(cx, elsewhere), before);

    // Once followed, the next tick reads nothing again.
    cx.update(|_, cx| view.update(cx, |view, cx| view.poll_reviews(cx)));
    assert_eq!(
        view.read_with(cx, |view, _| view.reviews[&id].request),
        request
    );
}

#[gpui::test]
fn a_review_of_a_vanished_workspace_is_left_alone(cx: &mut gpui::TestAppContext) {
    let (view, cx) = window(cx, None);
    cx.update(|window, cx| view.update(cx, |view, cx| view.seed_review(changes(), window, cx)));
    worktree_on(&view, cx, "local/preview");
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            let mut shown = (**view.live.snapshot.as_ref().unwrap()).clone();
            shown
                .workspaces
                .retain(|workspace| workspace.workspace_id != "w0");
            view.live.snapshot = Some(Arc::new(shown));
        })
    });
    let id = view.read_with(cx, |view, _| the(view));
    let before = saved(cx, id);
    let (checkout, request) = view.read_with(cx, |view, _| {
        let review = &view.reviews[&id];
        (review.checkout.clone(), review.request)
    });

    cx.update(|_, cx| view.update(cx, |view, cx| view.poll_reviews(cx)));

    assert_eq!(saved(cx, id), before);
    view.read_with(cx, |view, _| {
        let review = &view.reviews[&id];
        assert_eq!(review.checkout, checkout);
        assert_eq!(review.request, request);
    });
}

#[gpui::test]
fn a_remembered_checkout_is_not_pulled_back_by_the_daemon_s_branch(cx: &mut gpui::TestAppContext) {
    let (view, cx) = window(cx, None);
    cx.update(|window, cx| view.update(cx, |view, cx| view.seed_review(changes(), window, cx)));
    let id = view.read_with(cx, |view, _| the(view));
    // The worktree moved on; the daemon, not showing branches, still reports
    // the branch it had.
    let worktree = std::env::temp_dir().join("repo-worktree");
    reviewing(
        &view,
        cx,
        id,
        Input {
            checkout: Some(worktree.to_str().unwrap().into()),
            repo_key: Some(repo_key()),
            branch: "local/preview".into(),
        },
    );
    worktree_on(&view, cx, "feature");
    let before = saved(cx, id);
    let request = view.read_with(cx, |view, _| view.reviews[&id].request);

    cx.update(|_, cx| view.update(cx, |view, cx| view.poll_reviews(cx)));

    assert_eq!(saved(cx, id), before);
    assert_eq!(
        view.read_with(cx, |view, _| view.reviews[&id].request),
        request
    );
}
