use super::*;
use crate::doc_view::links::{FileLink, Form, Key};
use std::path::PathBuf;

fn file(path: &str, line: Option<u32>) -> FileLink {
    FileLink {
        path: PathBuf::from(path),
        line,
    }
}

#[test]
fn markdown_file_links_only_push_targets_for_existing_files() {
    let text = "[plan](docs/plan.md:12) [missing](missing.md) [web](https://example.com)";
    let lines = markdown::lines(text);
    let files = |key: Key| match (key.form, key.text) {
        (Form::Target, "docs/plan.md:12") => Some(file("docs/plan.md", Some(12))),
        _ => None,
    };
    let row = row(text, &lines[0], &look(), false, &[], &files);
    let web_start = text.find("[web]").unwrap();
    assert_eq!(
        row.links,
        vec![
            (1..5, Target::File(file("docs/plan.md", Some(12)))),
            (
                web_start + 1..web_start + 4,
                Target::Url("https://example.com".into())
            ),
        ]
    );
}

#[test]
fn prose_file_links_use_their_source_ranges() {
    let text = "see src/main.rs:12 and `src/other.rs`";
    let lines = markdown::lines(text);
    let files = |key: Key| match (key.form, key.text) {
        (Form::Prose, "src/main.rs:12") => Some(file("src/main.rs", Some(12))),
        (Form::Code, "src/other.rs") => Some(file("src/other.rs", None)),
        _ => None,
    };
    let row = row(text, &lines[0], &look(), false, &[], &files);
    assert_eq!(
        row.links,
        vec![
            (4..18, Target::File(file("src/main.rs", Some(12)))),
            (24..36, Target::File(file("src/other.rs", None))),
        ]
    );
}

#[test]
fn missing_markdown_file_links_keep_their_paint_without_a_target() {
    let text = "[x](missing.md)";
    let lines = markdown::lines(text);
    let row = row(text, &lines[0], &look(), false, &[], &|_| None);
    assert!(row.links.is_empty());
    assert!(row.runs.iter().any(|run| run.underline.is_some()));
}
