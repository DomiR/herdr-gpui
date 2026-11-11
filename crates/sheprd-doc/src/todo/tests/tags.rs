use super::*;

#[test]
fn extracts_normalized_tags_in_first_seen_order() {
    assert_eq!(
        tags("Ship @High @due(2026-11-10) @done (26-11-05 15:40) @high"),
        vec!["high", "due", "done"]
    );
    assert_eq!(tags("run `x` @code"), vec!["code"]);
}

#[test]
fn requires_whitespace_before_a_tag_and_bounds_names() {
    assert!(tags("a@b.com").is_empty());
    assert!(tags(&format!("@{}", "x".repeat(MAX_TAG + 1))).is_empty());
    assert_eq!(tag_name("  @High "), Some("high".into()));
    assert!(tag_name("@high.tag").is_none());
    assert!(tag_name("@").is_none());
}

#[test]
fn todos_include_their_tags() {
    let todo = todos("- [ ] Ship @High\n")[0].clone();
    assert_eq!(todo.tags, vec!["high"]);
}
