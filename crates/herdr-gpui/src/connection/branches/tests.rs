#![allow(clippy::unwrap_used)]

use super::*;
use herdr_client::protocol::ClientShellWorktree;
use std::{path::PathBuf, process::Command};

fn git(directory: &Path, args: &[&str]) {
    let output = Command::new("git")
        .arg("-C")
        .arg(directory)
        .args(args)
        .output()
        .unwrap();
    assert!(output.status.success(), "fixture git failed: {output:?}");
}

/// A repository with a linked worktree made on `worktree/tree` and renamed
/// to `feature/ticket` since, as Herdr still reports it.
fn renamed() -> (tempfile::TempDir, PathBuf, PathBuf) {
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
    (directory, repo, tree)
}

fn snapshot(cwd: &Path, key: &Path) -> ClientEvent {
    let mut snapshot = crate::sidebar::layout_tests::snapshot(1);
    let workspace = &mut snapshot.workspaces[0];
    workspace.new_workspace_cwd = cwd.to_str().unwrap().into();
    workspace.branch = Some("worktree/tree".into());
    workspace.worktree = Some(ClientShellWorktree {
        key: key.to_str().unwrap().into(),
        label: "repo".into(),
        is_linked_worktree: true,
    });
    ClientEvent::Snapshot(Arc::new(snapshot))
}

fn branch(event: &ClientEvent) -> Option<&str> {
    let ClientEvent::Snapshot(snapshot) = event else {
        panic!("not a snapshot");
    };
    snapshot.workspaces[0].branch.as_deref()
}

#[test]
fn a_renamed_branch_replaces_the_one_herdr_kept() {
    let (_directory, repo, tree) = renamed();
    let mut branches = Branches::default();
    let now = Instant::now();
    let event = branches.apply(snapshot(&tree, &repo.join(".git")), now);
    assert_eq!(branch(&event), Some("feature/ticket"));
    // A subdirectory of the checkout, where a pane may have gone, too.
    std::fs::create_dir(tree.join("src")).unwrap();
    let event = branches.apply(snapshot(&tree.join("src"), &repo.join(".git")), now);
    assert_eq!(branch(&event), Some("feature/ticket"));
}

#[test]
fn another_repository_or_a_detached_head_keeps_the_daemons_branch() {
    let (directory, repo, tree) = renamed();
    let mut branches = Branches::default();
    let now = Instant::now();
    // The shell is in a checkout of a repository other than Herdr's key.
    let other = directory.path().join("other");
    std::fs::create_dir(&other).unwrap();
    git(
        &other,
        &["init", "--quiet", "--template=", "-b", "elsewhere"],
    );
    let event = branches.apply(snapshot(&other, &repo.join(".git")), now);
    assert_eq!(branch(&event), Some("worktree/tree"));
    // Detached, there is no branch to correct it to.
    git(&tree, &["checkout", "--quiet", "--detach"]);
    let event = branches.apply(snapshot(&tree, &repo.join(".git")), now);
    assert_eq!(branch(&event), Some("worktree/tree"));
}

#[test]
fn a_read_answers_for_a_moment_then_reads_again() {
    let (_directory, repo, tree) = renamed();
    let mut branches = Branches::default();
    let key = repo.join(".git");
    let now = Instant::now();
    let event = branches.apply(snapshot(&tree, &key), now);
    assert_eq!(branch(&event), Some("feature/ticket"));
    git(&tree, &["branch", "-m", "feature/renamed-again"]);
    let event = branches.apply(snapshot(&tree, &key), now + Duration::from_millis(500));
    assert_eq!(branch(&event), Some("feature/ticket"));
    let event = branches.apply(snapshot(&tree, &key), now + FRESH);
    assert_eq!(branch(&event), Some("feature/renamed-again"));
}

#[test]
fn other_events_pass_through() {
    let mut branches = Branches::default();
    let event = branches.apply(
        ClientEvent::Disconnected {
            reason: "gone".into(),
            ssh: None,
        },
        Instant::now(),
    );
    assert!(matches!(event, ClientEvent::Disconnected { .. }));
}
