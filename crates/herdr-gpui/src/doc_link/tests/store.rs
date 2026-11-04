use super::*;
use crate::browser::Scope;
use std::fs;

fn record(path: &str) -> Record {
    Record {
        agent_name: "doc-notes-000001".into(),
        enabled: true,
        pane_id: "pane-1".into(),
        path: PathBuf::from(path),
        scope: Scope::endpoint("local"),
        session: None,
        tab_id: "tab-1".into(),
        workspace_id: "workspace-1".into(),
    }
}

fn json(record: serde_json::Value) -> Vec<u8> {
    serde_json::json!({"version": 1, "links": [record]})
        .to_string()
        .into_bytes()
}

#[test]
fn records_round_trip_through_the_store() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join(FILE);
    let links = vec![record("/tmp/notes.md")];
    save(&file, &links).unwrap();
    assert_eq!(load(&file).unwrap(), links);
    let bytes = fs::read(file).unwrap();
    assert_eq!(parse(&bytes).unwrap(), links);
}

#[test]
fn a_missing_store_is_empty() {
    let dir = tempfile::tempdir().unwrap();
    assert!(load(&dir.path().join(FILE)).unwrap().is_empty());
}

#[test]
fn malformed_json_and_schema_are_rejected() {
    assert!(matches!(parse(b"{"), Err(Error::Decode(_))));
    assert!(matches!(
        parse(br#"{"version":1,"links":[1]}"#),
        Err(Error::InvalidRecord {
            reason: RecordError::Malformed(_)
        })
    ));
    let bytes = serde_json::json!({"version": 2, "links": []})
        .to_string()
        .into_bytes();
    assert!(matches!(parse(&bytes), Err(Error::Schema { version: 2 })));
}

#[test]
fn every_record_limit_and_identity_rejection_is_typed() {
    assert!(matches!(
        parse(&json(serde_json::json!({
            "agent_name": "bad.name",
            "enabled": true,
            "pane_id": "pane",
            "path": "/tmp/a.md",
            "scope": "local",
            "session": null,
            "tab_id": "tab",
            "workspace_id": "workspace"
        }))),
        Err(Error::InvalidRecord {
            reason: RecordError::InvalidAgentName
        })
    ));
    assert!(matches!(
        parse(&json(serde_json::json!({
            "agent_name": "doc-a-000001",
            "enabled": true,
            "pane_id": "",
            "path": "/tmp/a.md",
            "scope": "local",
            "session": null,
            "tab_id": "tab",
            "workspace_id": "workspace"
        }))),
        Err(Error::InvalidRecord {
            reason: RecordError::EmptyField { field: "pane_id" }
        })
    ));
    assert!(matches!(
        parse(&json(serde_json::json!({
            "agent_name": "doc-a-000001",
            "enabled": true,
            "pane_id": "pane",
            "path": "relative.md",
            "scope": "local",
            "session": null,
            "tab_id": "tab",
            "workspace_id": "workspace"
        }))),
        Err(Error::InvalidRecord {
            reason: RecordError::RelativePath
        })
    ));
}

#[test]
fn duplicate_paths_and_string_bounds_are_rejected() {
    let mut first = record("/tmp/a.md");
    let mut second = record("/tmp/a.md");
    second.agent_name = "doc-b-000002".into();
    let bytes = serde_json::json!({
        "version": 1,
        "links": [first, second]
    })
    .to_string()
    .into_bytes();
    assert!(matches!(
        parse(&bytes),
        Err(Error::InvalidRecord {
            reason: RecordError::DuplicatePath
        })
    ));
    first.path = PathBuf::from("/tmp/a\n.md");
    assert!(matches!(
        save(Path::new("/tmp/doc-agent-links.json"), &[first]),
        Err(Error::InvalidRecord {
            reason: RecordError::ControlCharacter { field: "path" }
        })
    ));
    for field in ["tab_id", "workspace_id"] {
        let mut link = record("/tmp/b.md");
        if field == "tab_id" {
            link.tab_id.clear();
        } else {
            link.workspace_id.clear();
        }
        assert!(matches!(
            save(Path::new("/tmp/doc-agent-links.json"), &[link]),
            Err(Error::InvalidRecord {
                reason: RecordError::EmptyField { .. }
            })
        ));
    }
    let mut link = record("/tmp/c.md");
    link.session = Some("bad\tvalue".into());
    assert!(matches!(
        save(Path::new("/tmp/doc-agent-links.json"), &[link]),
        Err(Error::InvalidRecord {
            reason: RecordError::ControlCharacter { field: "session" }
        })
    ));
}

#[test]
fn too_many_links_and_oversized_fields_are_rejected() {
    let links = (0..=MAX_LINKS)
        .map(|number| {
            let mut link = record(&format!("/tmp/{number}.md"));
            link.agent_name = format!("doc-a-{number:06x}");
            link
        })
        .collect::<Vec<_>>();
    assert!(matches!(
        save(Path::new("/tmp/doc-agent-links.json"), &links),
        Err(Error::TooMany { limit: MAX_LINKS })
    ));
    let mut link = record("/tmp/a.md");
    link.pane_id = "x".repeat(257);
    assert!(matches!(
        save(Path::new("/tmp/doc-agent-links.json"), &[link]),
        Err(Error::InvalidRecord {
            reason: RecordError::FieldTooLong {
                field: "pane_id",
                ..
            }
        })
    ));
    let mut link = record("/tmp/a.md");
    link.path = PathBuf::from(format!("/{}", "x".repeat(4096)));
    assert!(matches!(
        save(Path::new("/tmp/doc-agent-links.json"), &[link]),
        Err(Error::InvalidRecord {
            reason: RecordError::PathTooLong { .. }
        })
    ));
}
