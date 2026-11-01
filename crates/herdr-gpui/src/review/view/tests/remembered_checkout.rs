//! A review found through Git's worktree list remembers the checkout with
//! its tab, and follows that checkout's own HEAD onto another branch, even
//! when the daemon never reports the switch. A detached HEAD keeps the
//! error, and a remembered path that is now another repository is never
//! read.
use super::{HerdrWindow, State, any_repository::git, window};
use crate::{
    browser::{Location, ReviewCheckout, Store, TabId, scope},
    pull_request::Input,
};
use gpui::{Entity, VisualTestContext};
use std::path::{Path, PathBuf};

/// A repository on `main` with one commit, and its Git directory.
fn repository(directory: &Path, branch: &str) -> String {
    std::fs::create_dir_all(directory.join("src")).unwrap();
    git(directory, &["init", "-q", "-b", branch]);
    std::fs::write(directory.join("src/lib.rs"), "fn a() {}\n").unwrap();
    git(directory, &["add", "-A"]);
    git(directory, &["commit", "-qm", "base"]);
    directory.join(".git").to_str().unwrap().into()
}

/// A repository with a worktree on `feature/old`: the repository's key and
/// the worktree.
fn with_worktree(root: &Path) -> (String, PathBuf) {
    let key = repository(&root.join("repo"), "main");
    let worktree = root.join("worktree");
    git(
        &root.join("repo"),
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            "feature/old",
            worktree.to_str().unwrap(),
        ],
    );
    (key, worktree)
}

/// Opens workspace `w0`'s review of `checkout` as a restored tab and waits
/// for its read.
fn opened(view: &Entity<HerdrWindow>, cx: &mut VisualTestContext, checkout: Input) -> TabId {
    let id = cx.update(|_, cx| {
        let tab_scope = scope(&view.read(cx).endpoints[0]);
        Store::update(cx, |store| {
            store.open(
                tab_scope,
                "w0",
                Some(Location::Review {
                    checkout: ReviewCheckout::try_from(&checkout).unwrap(),
                }),
                None,
            )
        })
        .unwrap()
    });
    cx.update(|_, cx| view.update(cx, |view, cx| view.poll_reviews(cx)));
    cx.run_until_parked();
    id
}

fn reload(view: &Entity<HerdrWindow>, cx: &mut VisualTestContext, id: TabId) {
    cx.update(|_, cx| view.update(cx, |view, cx| view.reload_review(id, cx)));
    cx.run_until_parked();
}

/// The tab's saved checkout and title.
fn saved(cx: &mut VisualTestContext, id: TabId) -> (Input, String) {
    cx.update(|_, cx| {
        let tab = cx.global::<Store>().get(id).unwrap();
        let Some(Location::Review { checkout }) = &tab.location else {
            panic!("not a review: {tab:?}");
        };
        (Input::from(checkout), tab.title.clone())
    })
}

/// The files the review lists, or what it failed with.
fn listed(
    view: &Entity<HerdrWindow>,
    cx: &mut VisualTestContext,
    id: TabId,
) -> Result<Vec<String>, String> {
    view.read_with(cx, |view, _| match &view.reviews[&id].state {
        State::Loaded(loaded) => Ok(loaded
            .diff
            .files
            .iter()
            .map(|file| file.path.clone())
            .collect()),
        State::Failed(error) => Err(error.clone()),
        State::Loading => panic!("the review is still loading"),
    })
}

fn same_place(a: &str, b: &Path) -> bool {
    Path::new(a).canonicalize().unwrap() == b.canonicalize().unwrap()
}

#[gpui::test]
fn a_worktree_switched_to_another_branch_is_followed(cx: &mut gpui::TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let (key, worktree) = with_worktree(root.path());
    let (view, cx) = window(cx, None);
    let id = opened(
        &view,
        cx,
        Input {
            checkout: None,
            repo_key: Some(key.clone()),
            branch: "feature/old".into(),
        },
    );
    assert_eq!(listed(&view, cx, id), Ok(Vec::new()));
    // Found through the worktree list, the checkout is kept with the tab.
    let (remembered, _) = saved(cx, id);
    let path = remembered.checkout.clone().unwrap();
    assert!(same_place(&path, &worktree), "{path}");
    assert_eq!(remembered.repo_key.as_deref(), Some(key.as_str()));

    // No worktree has the saved branch any more; the daemon is not told.
    git(&worktree, &["switch", "-q", "-c", "local/preview"]);
    std::fs::write(worktree.join("src/lib.rs"), "fn a() { todo!() }\n").unwrap();
    reload(&view, cx, id);

    let followed = Input {
        checkout: Some(path),
        repo_key: Some(key),
        branch: "local/preview".into(),
    };
    assert_eq!(
        saved(cx, id),
        (followed.clone(), "Review \u{00b7} local/preview".into())
    );
    assert_eq!(listed(&view, cx, id), Ok(vec!["src/lib.rs".to_owned()]));
    assert_eq!(
        view.read_with(cx, |view, _| view.reviews[&id].checkout.clone()),
        followed
    );

    // A detached HEAD, as in a rebase, keeps the error and the tab.
    git(&worktree, &["switch", "-q", "--detach"]);
    reload(&view, cx, id);
    assert!(listed(&view, cx, id).is_err());
    assert_eq!(saved(cx, id).0, followed);
}

#[gpui::test]
fn a_remembered_path_in_another_repository_is_refused(cx: &mut gpui::TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let key = repository(&root.path().join("repo"), "main");
    // Another repository now sits where the checkout was, on the saved branch.
    let other = root.path().join("other");
    repository(&other, "feature/old");
    std::fs::write(other.join("src/lib.rs"), "fn secret() {}\n").unwrap();
    let (view, cx) = window(cx, None);
    let id = opened(
        &view,
        cx,
        Input {
            checkout: Some(other.to_str().unwrap().into()),
            repo_key: Some(key.clone()),
            branch: "feature/old".into(),
        },
    );

    // Its changes are never listed; the repository's own worktrees are
    // asked instead, and none has the branch.
    assert!(listed(&view, cx, id).is_err());
    assert_eq!(
        saved(cx, id).0,
        Input {
            checkout: None,
            repo_key: Some(key),
            branch: "feature/old".into(),
        }
    );
}
