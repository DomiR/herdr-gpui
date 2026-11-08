use super::*;

#[test]
fn references_parse_all_line_forms() {
    for (text, line) in [
        ("src/main.rs:12", Some(12)),
        ("src/main.rs:12:5", Some(12)),
        ("src/main.rs#L12", Some(12)),
        ("src/main.rs#L12-L15", Some(12)),
        ("src/main.rs#L12C4", Some(12)),
        ("src/main.rs#L12C4-L15C2", Some(12)),
    ] {
        assert_eq!(reference::parse(text, false).unwrap().line, line, "{text}");
    }
}

#[test]
fn references_decode_targets_and_home_paths() {
    let decoded = reference::parse("<docs/my%20plan.md>", true).unwrap();
    assert_eq!(decoded.path, "docs/my plan.md");
    assert!(!decoded.home);
    let home = reference::parse("~/notes/x.md", false).unwrap();
    assert_eq!(home.path, "notes/x.md");
    assert!(home.home);
    assert_eq!(
        reference::parse("file:///tmp/my%20plan.md", false)
            .unwrap()
            .path,
        "/tmp/my plan.md"
    );
    assert_eq!(
        reference::parse("file://localhost/tmp/x.md", false)
            .unwrap()
            .path,
        "/tmp/x.md"
    );
}

#[test]
fn invalid_references_are_rejected() {
    for text in [
        "https://example.com/x",
        "mailto:a@example.com",
        "ftp://example.com/x",
        "#anchor",
        "-flag",
        "",
    ] {
        assert!(reference::parse(text, true).is_none(), "{text}");
    }
}

#[test]
fn probing_uses_roots_and_home_without_reading_process_home() {
    let roots_dir = tempfile::tempdir().unwrap();
    let home_dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(roots_dir.path().join("src")).unwrap();
    std::fs::create_dir_all(home_dir.path().join("notes")).unwrap();
    std::fs::write(roots_dir.path().join("src/main.rs"), "").unwrap();
    let absolute = roots_dir.path().join("absolute.md");
    std::fs::write(&absolute, "").unwrap();
    std::fs::write(home_dir.path().join("notes/x.md"), "").unwrap();
    let roots = Roots {
        dirs: vec![roots_dir.path().to_owned()],
        home: Some(home_dir.path().to_owned()),
    };
    assert_eq!(
        probe(&roots, Form::Target, "src/main.rs#L12").unwrap().line,
        Some(12)
    );
    assert_eq!(
        probe(&roots, Form::Target, "~/notes/x.md").unwrap().path,
        home_dir.path().join("notes/x.md")
    );
    let file_url = if cfg!(windows) {
        format!("file:///{}", absolute.display())
    } else {
        format!("file://{}", absolute.display())
    };
    assert_eq!(
        probe(&roots, Form::Target, &file_url).unwrap().path,
        absolute
    );
    assert!(probe(&roots, Form::Target, "missing.md").is_none());
}
