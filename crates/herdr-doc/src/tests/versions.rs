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
