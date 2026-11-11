use super::*;
use sheprd_doc::markdown;

fn ranges(text: &str) -> Vec<&str> {
    let lines = markdown::lines(text);
    bare_paths(text, &lines[0])
        .into_iter()
        .map(|range| &text[range])
        .collect()
}

#[test]
fn bare_paths_are_trimmed_and_suffixes_are_kept() {
    assert_eq!(
        ranges("see src/main.rs:12 and ~/x.md, README.md. README.md:12 main.rs#L12"),
        [
            "src/main.rs:12",
            "~/x.md",
            "README.md",
            "README.md:12",
            "main.rs#L12"
        ]
    );
}

#[test]
fn prose_skips_code_links_urls_and_non_paths() {
    assert_eq!(
        ranges("`src/no.rs` [src/no.rs](src/no.rs) https://x.y/z a@b.com e.g. 1.2.3"),
        Vec::<&str>::new()
    );
    let lines = markdown::lines("```\nsrc/no.rs\n```");
    assert!(bare_paths("```\nsrc/no.rs\n```", &lines[1]).is_empty());
}
