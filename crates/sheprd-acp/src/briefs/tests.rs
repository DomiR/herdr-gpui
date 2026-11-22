use super::*;

#[test]
fn a_brief_is_known_by_its_text_as_shown() {
    let sum = checksum("Ship it");
    assert_eq!(checksum("  Ship it\n"), sum);
    assert_eq!(
        checksum("<pasted_content id=\"1\">\nShip it\n</pasted_content>"),
        sum
    );
    assert_ne!(checksum("Ship it now"), sum);
}

#[test]
fn the_brief_file_is_read_once_and_removed() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("brief.md");
    fs::write(&path, "Do the thing").unwrap();
    let brief = take(&path, Some(Agent::Claude)).unwrap();
    assert_eq!(brief.text, "Do the thing");
    assert_eq!(brief.from, Some(Agent::Claude));
    assert!(!path.exists());
    assert!(matches!(take(&path, None), Err(Error::Read { .. })));
}

#[test]
fn a_brief_file_that_cannot_be_a_prompt_is_refused_and_still_removed() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("brief.md");
    fs::write(&path, vec![b'a'; MAX_PROMPT_SIZE + 1]).unwrap();
    assert!(matches!(take(&path, None), Err(Error::TooLong)));
    assert!(!path.exists());
    fs::write(&path, " \n").unwrap();
    assert!(matches!(take(&path, None), Err(Error::Empty)));
    fs::write(&path, [0xff, 0xfe]).unwrap();
    assert!(matches!(take(&path, None), Err(Error::NotUtf8(_))));
}

#[test]
fn briefs_are_kept_per_session_privately_and_bounded() {
    let dir = tempfile::tempdir().unwrap();
    let dir = dir.path().join("briefs");
    for index in 0..MAX_BRIEFS + 2 {
        save(&dir, "s-1", Known::of(&format!("brief {index}"), None)).unwrap();
    }
    let kept = load(&dir, "s-1");
    assert_eq!(kept.len(), MAX_BRIEFS);
    // The oldest went first.
    assert_eq!(kept[0], Known::of("brief 2", None));
    assert!(load(&dir, "s-2").is_empty());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        let mode = fs::metadata(dir.join("s-1.json"))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
    }
}

#[test]
fn a_session_id_that_cannot_name_a_file_keeps_nothing() {
    let dir = tempfile::tempdir().unwrap();
    for session in ["", "../escape", "a/b", &"x".repeat(MAX_SESSION_ID + 1)] {
        save(dir.path(), session, Known::of("brief", None)).unwrap();
        assert!(load(dir.path(), session).is_empty(), "{session}");
    }
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 0);
}

#[test]
fn a_file_that_no_longer_decodes_or_is_too_large_marks_nothing() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("s.json"), "not json").unwrap();
    assert!(load(dir.path(), "s").is_empty());
    fs::write(dir.path().join("s.json"), vec![b' '; MAX_FILE as usize + 1]).unwrap();
    assert!(load(dir.path(), "s").is_empty());
    // Saving over it starts again.
    save(dir.path(), "s", Known::of("brief", None)).unwrap();
    assert_eq!(load(dir.path(), "s").len(), 1);
}

#[test]
fn a_checksum_is_written_as_sixteen_hex_digits() {
    let file = File {
        briefs: vec![Known {
            from: Some(Agent::Claude),
            sum: 0xab,
        }],
    };
    assert_eq!(
        serde_json::to_string(&file).unwrap(),
        r#"{"briefs":[{"from":"claude","sum":"00000000000000ab"}]}"#
    );
}

#[test]
fn the_oldest_session_files_go_past_the_limit() {
    let dir = tempfile::tempdir().unwrap();
    for index in 0..MAX_FILES {
        fs::write(dir.path().join(format!("old-{index}.json")), "{}").unwrap();
    }
    save(dir.path(), "new", Known::of("brief", None)).unwrap();
    let count = fs::read_dir(dir.path()).unwrap().count();
    assert_eq!(count, MAX_FILES);
    assert_eq!(load(dir.path(), "new").len(), 1);
}
