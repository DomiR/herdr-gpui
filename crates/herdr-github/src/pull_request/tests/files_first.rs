//! A checkout Git's files settle is resolved without a Git process; one they
//! do not is left to Git, which names what is wrong.

use super::*;
use crate::pull_request::fetch::checkout_from_files;
use std::path::Path;

fn git(directory: &Path, args: &[&str]) {
    let mut command = Command::new("git");
    command.arg("-C").arg(directory).args(args);
    let (ok, text) = run(&mut command, Instant::now() + TIMEOUT, &|| false).unwrap();
    assert!(ok, "fixture git failed: {text}");
}

#[test]
fn a_renamed_linked_worktree_is_found_by_its_new_branch_from_files() {
    let directory = tempfile::tempdir().unwrap();
    let repo = directory.path().join("repo");
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
    let tree = directory.path().join("tree");
    git(
        &repo,
        &[
            "worktree",
            "add",
            "--quiet",
            "-b",
            "worktree/tree",
            tree.to_str().unwrap(),
        ],
    );
    git(&tree, &["branch", "-m", "feature/ticket"]);
    let key = repo.join(".git");
    let key = key.to_str().unwrap();
    let mut input = Input {
        checkout: None,
        repo_key: Some(key.into()),
        branch: "feature/ticket".into(),
    };
    let found = checkout_from_files(&input, key).unwrap();
    assert_eq!(
        Path::new(&found).canonicalize().unwrap(),
        tree.canonicalize().unwrap()
    );
    // The name Herdr kept from before the rename is on no worktree.
    input.branch = "worktree/tree".into();
    assert_eq!(checkout_from_files(&input, key), None);
    // A directory checks against its repository and branch too.
    input.checkout = Some(tree.join(".").to_str().unwrap().into());
    assert_eq!(checkout_from_files(&input, key), None);
    input.branch = "feature/ticket".into();
    assert!(checkout_from_files(&input, key).is_some());
    let other = directory.path().join("other");
    std::fs::create_dir(&other).unwrap();
    git(
        &other,
        &["init", "--quiet", "--template=", "-b", "feature/ticket"],
    );
    let other_key = other.join(".git");
    assert_eq!(
        checkout_from_files(&input, other_key.to_str().unwrap()),
        None
    );
}
