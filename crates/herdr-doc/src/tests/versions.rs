use super::*;

#[test]
fn every_change_takes_the_next_number() {
    let mut doc = Document::new("a\n").unwrap();
    assert_eq!(doc.version(), Version(1));
    type_at(&mut doc, 0, "b");
    assert_eq!(doc.version(), Version(2));
    doc.merge_disk("c\n").unwrap();
    assert_eq!(doc.version(), Version(3));
    // An edit that changes nothing is no new version.
    let c = doc.text().find('c').unwrap();
    doc.edit(&[Edit::new(c..c + 1, "c")]).unwrap();
    doc.merge_disk("c\n").unwrap();
    assert_eq!(doc.version(), Version(3));
    let reopened = Document::at("x", Version(41)).unwrap();
    assert_eq!(reopened.version(), Version(41));
}

#[test]
fn a_stale_version_gets_a_diff_and_changes_nothing() {
    let mut doc = Document::new("one\ntwo\n").unwrap();
    type_at(&mut doc, 4, "TWO ");
    let before = doc.text().to_owned();
    let result = doc.agent_edit(&agent("p"), at(Version(1)), |_| {
        Ok(vec![Edit::insert(0, "nope")])
    });
    let Err(Error::Conflict(conflict)) = result else {
        panic!("expected a conflict");
    };
    assert_eq!(doc.text(), before);
    assert_eq!(conflict.expected, Version(1));
    assert_eq!(conflict.current, Version(2));
    let diff = conflict.diff.unwrap();
    assert!(
        diff.contains("-two\n") && diff.contains("+TWO two\n"),
        "{diff}"
    );
}

#[test]
fn an_unknown_or_forgotten_version_gets_no_diff() {
    let mut doc = Document::new("a").unwrap();
    for _ in 0..version::KEPT + 2 {
        type_at(&mut doc, 0, "x");
    }
    let conflict = doc.check_version(Version(1)).unwrap_err();
    assert_eq!(conflict.diff, None);
    let conflict = doc.check_version(Version(999_999)).unwrap_err();
    assert_eq!(conflict.diff, None);
    assert!(doc.check_version(doc.version()).is_ok());
}

#[test]
fn versions_parse_as_numbers() {
    assert_eq!("12".parse::<Version>().unwrap(), Version(12));
    assert_eq!("v7".parse::<Version>().unwrap(), Version(7));
    assert!(matches!("x".parse::<Version>(), Err(Error::BadVersion)));
    assert_eq!(Version(3).to_string(), "3");
}

#[test]
fn changes_since_a_version() {
    let mut doc = Document::new("a\n").unwrap();
    assert_eq!(doc.changes_since(Version(1)), Some(String::new()));
    type_at(&mut doc, 2, "b\n");
    let diff = doc.changes_since(Version(1)).unwrap();
    assert!(diff.contains("+b"), "{diff}");
    assert_eq!(doc.changes_since(Version(9)), None);
}

#[test]
fn changes_name_who_made_them_and_can_keep_to_a_section() {
    let mut doc = Document::new("# A\n- [ ] a\n# B\n- [ ] b\n").unwrap();
    let start = doc.version();
    doc.agent_edit(&agent("w1:p2"), at(doc.version()), |text| {
        todo::find(text, "b").map(|t| todo::set_done(text, t.line, true).into_iter().collect())
    })
    .unwrap();
    // The app writes the agent's change before another program edits it.
    save(&mut doc);
    doc.merge_disk("# A\n- [ ] a\n# B\n- [x] b\n- [ ] c\n")
        .unwrap();
    let all = doc.changes(start, None).unwrap();
    assert_eq!(all.by, vec![Origin::Agent(agent("w1:p2")), Origin::Disk]);
    assert_eq!(all.from, start);
    assert_eq!(all.to, doc.version());
    let kinds: Vec<_> = all.todos.iter().map(|c| (c.kind, c.number)).collect();
    assert_eq!(
        kinds,
        vec![(todo::ChangeKind::Checked, 2), (todo::ChangeKind::Added, 3)]
    );
    // Section A did not change: an empty diff and no tasks.
    let a = doc.changes(start, Some("a")).unwrap();
    assert_eq!(a.diff.as_deref(), Some(""));
    assert!(a.todos.is_empty());
    let b = doc.changes(start, Some("b")).unwrap();
    assert!(b.diff.unwrap().contains("+- [ ] c"));
    // The user's own typing is the user's.
    type_at(&mut doc, 0, "x");
    let mine = doc.changes(Version(start.0 + 2), None).unwrap();
    assert_eq!(mine.by, vec![Origin::User]);
    assert!(doc.changes(Version(999), None).is_none());
}
