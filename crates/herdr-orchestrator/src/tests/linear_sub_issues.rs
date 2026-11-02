//! A sub-issue is created under its parent with Linear's `parentId`.
use crate::linear::NewIssue;
use serde_json::json;

#[test]
fn a_sub_issue_names_its_parent() {
    let issue = NewIssue {
        team: "t-eng".into(),
        title: "Write tests".into(),
        parent: Some("uuid-12".into()),
        ..NewIssue::default()
    };
    assert_eq!(
        issue.input(),
        json!({ "teamId": "t-eng", "title": "Write tests", "parentId": "uuid-12" })
    );
    // Without a parent, none is sent.
    let top = NewIssue {
        parent: None,
        ..issue
    };
    assert!(top.input().get("parentId").is_none());
}
