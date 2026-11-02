//! The most recently committed branches, read with gix in the order
//! `git for-each-ref --sort=-committerdate` prints them: how a Linear
//! issue's own branch is found among a repository's others.

use super::git::{repository, run};
use crate::{git, naming};
use std::{path::Path, process::Command, sync::atomic::AtomicBool};

/// Commits on a new branch `name`, off the base, at `seconds`.
fn branch_at(main: &Path, name: &str, seconds: i64) {
    run(main, &["checkout", "-q", "-b", name, "master"]);
    let date = format!("@{seconds} +0000");
    let output = Command::new("git")
        .arg("-C")
        .arg(main)
        .args(["-c", "user.name=t", "-c", "user.email=t@t"])
        .args(["-c", "commit.gpgsign=false"])
        .args(["commit", "-q", "--allow-empty", "-m", name])
        .env("GIT_COMMITTER_DATE", &date)
        .env("GIT_AUTHOR_DATE", &date)
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
}

#[test]
fn recent_branches_match_git_newest_first() {
    let dir = tempfile::tempdir().unwrap();
    let main = repository(dir.path());
    run(&main, &["branch", "-m", "master"]);
    branch_at(&main, "dominique/eng-12-fix", 2_000_000_000);
    branch_at(&main, "old", 1_000_000_000);
    branch_at(&main, "agent/x", 1_950_000_000);
    // A packed branch is read with the loose ones.
    run(&main, &["pack-refs", "--all"]);
    branch_at(&main, "newest", 2_100_000_000);
    let printed = run(
        &main,
        &[
            "for-each-ref",
            "--count=3",
            "--sort=-committerdate",
            "--format=%(refname:short)",
            "refs/heads/",
        ],
    );
    let expected: Vec<&str> = printed.lines().collect();
    assert_eq!(expected, ["newest", "dominique/eng-12-fix", "agent/x"]);
    assert_eq!(git::recent_branches(&main, 3).unwrap(), expected);
    assert_eq!(git::recent_branches(&main, 10).unwrap().len(), 5);
}

/// The branches that could name an item, as this machine reads them and as
/// an SSH host's `git` queries print them: those named here, then the most
/// recently committed to that hold an identifier, each once.
#[test]
fn branches_naming_items_list_the_named_then_the_recent_once() {
    let dir = tempfile::tempdir().unwrap();
    let main = repository(dir.path());
    run(&main, &["branch", "-m", "master"]);
    branch_at(&main, "dominique/eng-12-fix", 2_000_000_000);
    branch_at(&main, "agent/377-icons-0123abcd", 2_050_000_000);
    branch_at(&main, "feature", 2_100_000_000);
    let expected = ["agent/377-icons-0123abcd", "dominique/eng-12-fix"];
    let host = herdr_remote::teleport::Host::new(&herdr_client::ConnectTarget::Local).unwrap();
    let never = AtomicBool::new(false);
    let root = main.to_string_lossy();
    assert_eq!(
        naming::local_branches(&host, &root, &never).unwrap(),
        expected
    );
    let listed = naming::branch_queries()
        .iter()
        .map(|query| run(&main, &query.each_ref().map(String::as_str)))
        .collect::<Vec<_>>()
        .join("\n");
    assert_eq!(naming::ours(listed.lines()), expected);
}
