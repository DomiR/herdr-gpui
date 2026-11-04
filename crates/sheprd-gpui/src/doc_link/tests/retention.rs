use super::*;

fn record(number: usize, enabled: bool, root: &Path) -> Record {
    Record {
        agent_name: format!("doc-a-{number:06x}"),
        enabled,
        pane_id: format!("p{number}"),
        path: root.join(format!("{number}.md")),
        scope: crate::browser::Scope::endpoint("fixture"),
        session: None,
        tab_id: format!("t{number}"),
        workspace_id: "w0".into(),
    }
}

#[test]
fn saving_evicts_the_oldest_off_records_before_enabled_links() {
    let dir = tempfile::tempdir().unwrap();
    let records: Vec<_> = (0..MAX_LINKS + 2)
        .map(|n| record(n, n >= 3, dir.path()))
        .collect();
    let file = dir.path().join(FILE);
    save(&file, &records).unwrap();
    let saved = load(&file).unwrap();
    assert_eq!(saved, records[2..]);
    assert!(saved.iter().any(|record| !record.enabled));
}

#[test]
fn eight_enabled_records_win_over_any_off_record() {
    let dir = tempfile::tempdir().unwrap();
    let mut records: Vec<_> = (0..MAX_LINKS)
        .map(|n| record(n, true, dir.path()))
        .collect();
    records.push(record(MAX_LINKS, false, dir.path()));
    let file = dir.path().join(FILE);
    save(&file, &records).unwrap();
    assert_eq!(load(&file).unwrap(), records[..MAX_LINKS]);
    records[MAX_LINKS].enabled = true;
    assert!(matches!(save(&file, &records), Err(Error::TooMany { .. })));
}
