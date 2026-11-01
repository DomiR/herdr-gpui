use super::*;

const LIST: &str = "# Todo\n- [ ] alpha\n- [ ] beta\n";

#[test]
fn typing_survives_an_agent_rewriting_the_file() {
    let mut doc = Document::new(LIST).unwrap();
    // The user types after "alpha" while the agent's write is under way.
    let at = LIST.find("alpha").unwrap() + "alpha".len();
    type_at(&mut doc, at, " now");
    let disk = "# Todo\n- [ ] alpha\n- [x] beta\n- [ ] gamma\n";
    let merge = doc.merge_disk(disk).unwrap();
    assert_eq!(
        doc.text(),
        "# Todo\n- [ ] alpha now\n- [x] beta\n- [ ] gamma\n"
    );
    let Merge::Merged { inserted } = merge else {
        panic!("expected a merge");
    };
    let texts: Vec<&str> = inserted.iter().map(|r| &doc.text()[r.clone()]).collect();
    assert!(texts.contains(&"x"), "{texts:?}");
    assert!(texts.iter().any(|t| t.contains("gamma")), "{texts:?}");
    // The merged text differs from the disk, so it is written back.
    assert!(doc.dirty());
}

#[test]
fn a_second_disk_change_is_diffed_from_the_first_not_applied_twice() {
    let mut doc = Document::new(LIST).unwrap();
    type_at(&mut doc, 0, "> note\n");
    doc.merge_disk("# Todo\n- [ ] alpha\n- [ ] beta\n- [ ] one\n")
        .unwrap();
    doc.merge_disk("# Todo\n- [ ] alpha\n- [ ] beta\n- [ ] one\n- [ ] two\n")
        .unwrap();
    assert_eq!(
        doc.text(),
        "> note\n# Todo\n- [ ] alpha\n- [ ] beta\n- [ ] one\n- [ ] two\n"
    );
}

#[test]
fn overlapping_edits_keep_both_writers_text() {
    let mut doc = Document::new("one two three\n").unwrap();
    // Both insert at the same place.
    type_at(&mut doc, 4, "USER ");
    doc.merge_disk("one DISK two three\n").unwrap();
    let text = doc.text().to_owned();
    assert!(text.contains("USER "), "{text}");
    assert!(text.contains("DISK "), "{text}");
    assert!(
        text.starts_with("one ") && text.ends_with("two three\n"),
        "{text}"
    );
}

#[test]
fn a_word_changed_inside_a_deleted_line_does_not_bring_the_line_back() {
    let mut doc = Document::new("keep\nbeta line\nend\n").unwrap();
    // The user deletes the middle line; the agent edits a word in it.
    doc.edit(&[Edit::new(5..15, "")]).unwrap();
    doc.merge_disk("keep\nbeta LINE\nend\n").unwrap();
    let text = doc.text();
    assert!(
        text.starts_with("keep\n") && text.ends_with("end\n"),
        "{text}"
    );
    assert!(!text.contains("beta"), "{text}");
}

#[test]
fn the_documents_own_write_does_not_echo() {
    let mut doc = Document::new(LIST).unwrap();
    type_at(&mut doc, 0, "x");
    let written = save(&mut doc);
    let version = doc.version();
    assert_eq!(doc.merge_disk(&written).unwrap(), Merge::Unchanged);
    assert_eq!(doc.text(), written);
    assert_eq!(doc.version(), version);
    assert!(!doc.dirty());
}

#[test]
fn a_write_read_back_before_it_is_confirmed_does_not_echo() {
    let mut doc = Document::new(LIST).unwrap();
    type_at(&mut doc, 0, "a");
    let write = doc.to_write();
    // The user keeps typing while the write is in flight, and a poll reads
    // the written file back before the write is confirmed.
    type_at(&mut doc, 1, "b");
    assert_eq!(doc.merge_disk(write.text()).unwrap(), Merge::Unchanged);
    assert_eq!(doc.text(), format!("ab{LIST}"));
    assert_eq!(doc.synced().text(), write.text());
    doc.wrote(write);
    assert!(doc.dirty());
}

#[test]
fn two_agents_commands_both_land() {
    let mut doc = Document::new(LIST).unwrap();
    let version = doc.version();
    doc.agent_edit(&agent("p1"), at(version), |text| {
        todo::add(text, "from one", None).map(|edit| vec![edit])
    })
    .unwrap();
    // The second agent read the same version: it is told what changed.
    let stale = doc.agent_edit(&agent("p2"), at(version), |text| {
        todo::add(text, "from two", None).map(|edit| vec![edit])
    });
    let Err(Error::Conflict(conflict)) = stale else {
        panic!("expected a conflict, got {stale:?}");
    };
    assert_eq!(conflict.current, doc.version());
    assert!(conflict.diff.unwrap().contains("+- [ ] from one"));
    doc.agent_edit(&agent("p2"), at(conflict.current), |text| {
        todo::add(text, "from two", None).map(|edit| vec![edit])
    })
    .unwrap();
    assert_eq!(
        doc.text(),
        "# Todo\n- [ ] alpha\n- [ ] beta\n- [ ] from one\n- [ ] from two\n"
    );
}

#[test]
fn undo_reverts_only_the_users_edits() {
    let mut doc = Document::new("a\n").unwrap();
    type_at(&mut doc, 1, " typed");
    doc.break_undo();
    let version = doc.version();
    doc.agent_edit(&agent("p1"), at(version), |text| {
        Ok(vec![Edit::insert(text.len(), "agent\n")])
    })
    .unwrap();
    // The app writes the agent's change, then another program edits it.
    save(&mut doc);
    doc.merge_disk("a typed\nagent\ndisk\n").unwrap();
    assert_eq!(doc.text(), "a typed\nagent\ndisk\n");
    let changed = doc.undo().unwrap();
    assert_eq!(doc.text(), "a\nagent\ndisk\n");
    assert_eq!(changed, Some(1..1));
    assert!(!doc.can_undo());
    doc.redo().unwrap();
    assert_eq!(doc.text(), "a typed\nagent\ndisk\n");
}

#[test]
fn anchors_follow_other_writers() {
    let mut doc = Document::new("hello world\n").unwrap();
    let caret = doc.anchor(6, false).unwrap();
    doc.merge_disk("oh hello world\n").unwrap();
    assert_eq!(doc.resolve(&caret), Some(9));
    assert_eq!(&doc.text()[9..14], "world");
}

#[test]
fn edits_are_checked_and_bounded() {
    let mut doc = Document::new("héllo\n").unwrap();
    assert!(matches!(
        doc.edit(&[Edit::new(2..3, "x")]),
        Err(Error::BadRange { .. })
    ));
    assert!(matches!(
        doc.edit(&[Edit::new(0..99, "x")]),
        Err(Error::BadRange { .. })
    ));
    assert!(matches!(
        doc.edit(&[Edit::new(0..2, "x"), Edit::new(1..3, "y")]),
        Err(Error::BadRange { .. })
    ));
    let huge = "x".repeat(MAX_BYTES);
    assert!(matches!(
        doc.edit(&[Edit::insert(0, huge.as_str())]),
        Err(Error::TooLarge { .. })
    ));
    assert!(matches!(
        Document::new(&format!("{huge}y")),
        Err(Error::TooLarge { .. })
    ));
    assert!(matches!(
        doc.merge_disk(&format!("{huge}y")),
        Err(Error::TooLarge { .. })
    ));
    let version = doc.version();
    assert!(matches!(
        doc.agent_edit(&agent("p"), at(version), |_| Ok(vec![Edit::insert(
            0,
            huge.as_str()
        )])),
        Err(Error::TooLarge { .. })
    ));
    assert_eq!(doc.text(), "héllo\n");
}
