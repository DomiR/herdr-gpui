#![allow(clippy::unwrap_used)]
use super::*;

#[test]
fn a_brief_is_read_from_text_a_file_or_a_stream() {
    assert_eq!(read(BriefSource::Text("Fix it".into())).unwrap(), "Fix it");
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("brief.md");
    std::fs::write(&path, "# Task\nFix the relay\n").unwrap();
    assert_eq!(
        read(BriefSource::File(path.into_os_string())).unwrap(),
        "# Task\nFix the relay\n"
    );
    assert_eq!(from(&b"From a pipe"[..]).unwrap(), "From a pipe");
}

#[test]
fn a_blank_long_or_binary_brief_is_refused() {
    assert!(matches!(from(&b" \n "[..]), Err(Error::Empty)));
    assert!(matches!(
        read(BriefSource::Text(String::new())),
        Err(Error::Empty)
    ));
    let long = vec![b'a'; MAX_BRIEF + 1];
    assert!(matches!(from(&long[..]), Err(Error::TooLong)));
    assert!(matches!(
        read(BriefSource::Text("a".repeat(MAX_BRIEF + 1))),
        Err(Error::TooLong)
    ));
    assert_eq!(from(&vec![b'a'; MAX_BRIEF][..]).unwrap().len(), MAX_BRIEF);
    assert!(matches!(from(&[0xff, 0xfe][..]), Err(Error::Text)));
}

#[test]
fn a_missing_file_names_its_path() {
    let error = read(BriefSource::File("/nonexistent/brief.md".into())).unwrap_err();
    assert!(matches!(&error, Error::Open { path, .. } if path.ends_with("brief.md")));
    assert!(std::error::Error::source(&error).is_some());
}
