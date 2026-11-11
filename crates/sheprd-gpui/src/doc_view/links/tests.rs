#![allow(clippy::unwrap_used)]
use super::*;

mod prose;
mod references;

#[test]
fn paths_and_lines_are_told_from_other_code() {
    for yes in ["src/main.rs", "README.md", "docs/plan.md:12", "./a/b"] {
        assert!(looks_like_path(yes), "{yes}");
    }
    for no in ["x", "a b.rs", "https://a.b/c", "--flag", "foo()"] {
        assert!(!looks_like_path(no), "{no}");
    }
    assert_eq!(reference::parse("a.rs:12", false).unwrap().line, Some(12));
    assert_eq!(reference::parse("a.rs:x", false).unwrap().line, None);
    assert_eq!(reference::parse("a.rs", false).unwrap().line, None);
}

#[test]
fn spans_resolve_in_the_folder_then_the_checkout() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join(".git")).unwrap();
    std::fs::create_dir_all(dir.path().join("notes")).unwrap();
    std::fs::create_dir_all(dir.path().join("src")).unwrap();
    std::fs::write(dir.path().join("src/main.rs"), "").unwrap();
    std::fs::write(dir.path().join("notes/near.md"), "").unwrap();
    let roots = Roots {
        dirs: roots(&dir.path().join("notes/todo.md")),
        home: None,
    };
    assert_eq!(roots.dirs.len(), 2);
    assert_eq!(
        probe(&roots, Form::Code, "src/main.rs:3"),
        Some(FileLink {
            path: dir.path().join("src/main.rs"),
            line: Some(3)
        })
    );
    assert!(probe(&roots, Form::Code, "near.md").is_some());
    assert!(probe(&roots, Form::Code, "missing.rs").is_none());
}
