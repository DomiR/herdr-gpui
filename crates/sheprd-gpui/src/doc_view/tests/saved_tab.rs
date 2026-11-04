use super::*;

#[test]
fn a_saved_doc_file_is_checked_when_read() {
    let ok: DocFile =
        serde_json::from_str(r#"{"path":"/notes/todo.md","line":3,"folded":["goals"]}"#).unwrap();
    assert_eq!(ok.folded, vec!["goals".to_owned()]);
    assert_eq!(ok.name(), "todo.md");
    for bad in [
        r#"{"path":"relative.md","line":null}"#,
        r#"{"path":"/notes/todo.txt","line":null}"#,
        r#"{"path":"","line":null}"#,
    ] {
        assert!(serde_json::from_str::<DocFile>(bad).is_err(), "{bad}");
    }
    let plain = serde_json::to_string(&DocFile {
        path: "/a.md".into(),
        line: None,
        folded: Vec::new(),
    })
    .unwrap();
    assert_eq!(plain, r#"{"path":"/a.md","line":null}"#);
}

#[test]
fn markdown_files_are_recognized() {
    for yes in ["a.md", "B.MD", "c.markdown", "d.mdown"] {
        assert!(is_markdown(std::path::Path::new(yes)), "{yes}");
    }
    for no in ["a.txt", "md", "a.md.bak"] {
        assert!(!is_markdown(std::path::Path::new(no)), "{no}");
    }
}
