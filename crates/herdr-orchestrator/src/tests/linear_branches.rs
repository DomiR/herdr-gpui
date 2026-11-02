use super::*;
use crate::naming::{LiveBranch, branch, found_runs, hash_of};

fn eng12() -> Item {
    let source = SourceKey {
        provider: Provider::Linear,
        host: "linear.app".into(),
        repository: "acme".into(),
    };
    let mut issue = item(&source, "ENG-12", "Fix login");
    issue.identifier = "ENG-12".into();
    issue
}

#[test]
fn dispatch_uses_the_branch_linear_names() {
    let mut issue = eng12();
    issue.tracker = Some(Tracker {
        branch: Some("dominique/eng-12-fix-login".into()),
        ..Tracker::default()
    });
    assert_eq!(branch(&issue), "dominique/eng-12-fix-login");
    issue.tracker = Some(Tracker {
        branch: Some("bad branch..name".into()),
        ..Tracker::default()
    });
    assert_eq!(branch(&issue), "agent/eng-12-fix-login-fc657f77");
    issue.tracker = Some(Tracker::default());
    assert_eq!(branch(&issue), "agent/eng-12-fix-login-fc657f77");
}

fn workspace(id: &str, branch: &str) -> LiveBranch {
    LiveBranch {
        host: None,
        session: None,
        workspace_id: id.into(),
        branch: branch.into(),
    }
}

#[test]
fn branches_holding_an_identifier_are_that_issues_runs() {
    let issue = eng12();
    let other = item(&github(), "5", "Other");
    let items = [issue.clone(), other.clone()];
    let workspaces = [
        workspace("w1", "dominique/eng-12-fix"),
        workspace("w2", "ENG-12"),
        workspace("w3", "eng-123-x"),
        workspace("w4", "xeng-12"),
        workspace("w5", &format!("agent/eng-12-other-{}", hash_of(&other))),
    ];
    let branches = [
        "feature/eng-12".to_owned(),
        "dominique/eng-12-fix".to_owned(),
        "eng-1200".to_owned(),
    ];
    let found: Vec<(String, String)> = found_runs(&items, &[], &workspaces, &branches, &[])
        .into_iter()
        .map(|run| (run.id, run.item_key))
        .collect();
    let eng = issue.key.canonical();
    assert_eq!(
        found,
        [
            ("branch:None:None:w1".to_owned(), eng.clone()),
            ("branch:None:None:w2".to_owned(), eng.clone()),
            ("branch:None:None:w5".to_owned(), other.key.canonical()),
            ("branch:feature/eng-12".to_owned(), eng),
        ]
    );
}

#[test]
fn only_linear_items_are_found_by_identifier() {
    let mut bead = item(&beads(), "hg-12", "Bead");
    bead.identifier = "HG-12".into();
    let found = found_runs(&[bead], &[], &[workspace("w1", "hg-12-work")], &[], &[]);
    assert_eq!(found, []);
}
