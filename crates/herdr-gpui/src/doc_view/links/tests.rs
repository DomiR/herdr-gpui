#![allow(clippy::unwrap_used)]
use super::*;

#[test]
fn paths_and_lines_are_told_from_other_code() {
    for yes in ["src/main.rs", "README.md", "docs/plan.md:12", "./a/b"] {
        assert!(looks_like_path(yes), "{yes}");
    }
    for no in ["x", "a b.rs", "https://a.b/c", "--flag", "foo()"] {
        assert!(!looks_like_path(no), "{no}");
    }
    assert_eq!(path_and_line("a.rs:12"), ("a.rs", Some(12)));
    assert_eq!(path_and_line("a.rs:x"), ("a.rs:x", None));
    assert_eq!(path_and_line("a.rs"), ("a.rs", None));
}

#[test]
fn spans_resolve_in_the_folder_then_the_checkout() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join(".git")).unwrap();
    std::fs::create_dir_all(dir.path().join("notes")).unwrap();
    std::fs::create_dir_all(dir.path().join("src")).unwrap();
    std::fs::write(dir.path().join("src/main.rs"), "").unwrap();
    std::fs::write(dir.path().join("notes/near.md"), "").unwrap();
    let roots = roots(&dir.path().join("notes/todo.md"));
    assert_eq!(roots.len(), 2);
    assert_eq!(
        probe(&roots, "src/main.rs:3"),
        Some(FileLink {
            path: dir.path().join("src/main.rs"),
            line: Some(3)
        })
    );
    assert!(probe(&roots, "near.md").is_some());
    assert!(probe(&roots, "missing.rs").is_none());
}
