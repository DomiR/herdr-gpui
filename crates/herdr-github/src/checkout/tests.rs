#![allow(clippy::unwrap_used)]

use super::{Checkout, Found, checkout_at, find_branch};
use std::{
    path::{Path, PathBuf},
    process::Command,
};

fn git(directory: &Path, args: &[&str]) {
    let output = Command::new("git")
        .arg("-C")
        .arg(directory)
        .args(args)
        .output()
        .unwrap();
    assert!(output.status.success(), "fixture git failed: {output:?}");
}

/// A repository on `main` with one commit, so worktrees can branch off it.
fn repository(parent: &Path) -> PathBuf {
    let repo = parent.join("repo");
    std::fs::create_dir(&repo).unwrap();
    git(&repo, &["init", "--quiet", "--template=", "-b", "main"]);
    git(
        &repo,
        &[
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@t",
            "commit",
            "--quiet",
            "--allow-empty",
            "-m",
            "first",
        ],
    );
    repo.canonicalize().unwrap()
}

#[test]
fn a_main_checkout_and_its_subdirectories_read_as_it() {
    let directory = tempfile::tempdir().unwrap();
    let repo = repository(directory.path());
    let nested = repo.join("src/deep");
    std::fs::create_dir_all(&nested).unwrap();
    let expected = Checkout {
        root: repo.clone(),
        common: repo.join(".git"),
        branch: Some("main".into()),
    };
    assert_eq!(checkout_at(&repo), Some(expected.clone()));
    assert_eq!(checkout_at(&nested), Some(expected));
    assert_eq!(
        find_branch(repo.join(".git"), "main"),
        Some(Found::One(repo))
    );
}

#[test]
fn a_linked_worktree_reads_as_its_own_checkout_of_the_shared_repository() {
    let directory = tempfile::tempdir().unwrap();
    let repo = repository(directory.path());
    let linked = directory.path().join("tree");
    git(
        &repo,
        &[
            "worktree",
            "add",
            "--quiet",
            "-b",
            "worktree/tree",
            linked.to_str().unwrap(),
        ],
    );
    let linked = linked.canonicalize().unwrap();
    // Renamed after the worktree was made, as an agent adopting a ticket's
    // branch does: the files say the new name at once.
    git(&linked, &["branch", "-m", "feature/ticket"]);
    assert_eq!(
        checkout_at(&linked),
        Some(Checkout {
            root: linked.clone(),
            common: repo.join(".git"),
            branch: Some("feature/ticket".into()),
        })
    );
    let found = find_branch(repo.join(".git"), "feature/ticket").unwrap();
    let Found::One(path) = found else {
        panic!("{found:?}");
    };
    assert_eq!(path.canonicalize().unwrap(), linked);
    assert_eq!(
        find_branch(repo.join(".git"), "worktree/tree"),
        Some(Found::Missing)
    );
    // A detached worktree has no branch, and is on none.
    git(&linked, &["checkout", "--quiet", "--detach"]);
    assert_eq!(checkout_at(&linked).unwrap().branch, None);
    assert_eq!(
        find_branch(repo.join(".git"), "feature/ticket"),
        Some(Found::Missing)
    );
}

#[test]
fn a_relative_gitdir_line_resolves_from_the_checkout() {
    let directory = tempfile::tempdir().unwrap();
    let repo = repository(directory.path());
    let linked = directory.path().join("tree");
    git(
        &repo,
        &[
            "worktree",
            "add",
            "--quiet",
            "-b",
            "topic",
            linked.to_str().unwrap(),
        ],
    );
    let relative = Path::new("../repo/.git/worktrees/tree");
    std::fs::write(
        linked.join(".git"),
        format!("gitdir: {}\n", relative.display()),
    )
    .unwrap();
    let found = checkout_at(&linked).unwrap();
    assert_eq!(found.common, repo.join(".git"));
    assert_eq!(found.branch.as_deref(), Some("topic"));
}

#[test]
fn layouts_these_readers_do_not_know_are_left_to_git() {
    let directory = tempfile::tempdir().unwrap();
    let repo = repository(directory.path());
    // Reftable keeps this placeholder in HEAD; the branch is elsewhere.
    std::fs::write(repo.join(".git/HEAD"), "ref: refs/heads/.invalid\n").unwrap();
    assert_eq!(checkout_at(&repo), None);
    assert_eq!(find_branch(repo.join(".git"), "main"), None);
    // A symbolic HEAD outside local branches, and garbage.
    std::fs::write(repo.join(".git/HEAD"), "ref: refs/remotes/origin/main\n").unwrap();
    assert_eq!(checkout_at(&repo), None);
    std::fs::write(repo.join(".git/HEAD"), "not a head\n").unwrap();
    assert_eq!(checkout_at(&repo), None);
    // A Git directory not named `.git` does not say where its checkout is.
    let bare = directory.path().join("bare.git");
    let output = Command::new("git")
        .args(["init", "--quiet", "--bare", "--template=", "-b", "main"])
        .arg(&bare)
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(find_branch(&bare, "main"), None);
    // Outside any repository there is nothing to read.
    let plain = tempfile::tempdir().unwrap();
    if checkout_at(plain.path()).is_some() {
        // A temporary directory inside someone's checkout proves nothing.
        return;
    }
    assert_eq!(checkout_at(plain.path().join("missing")), None);
}
