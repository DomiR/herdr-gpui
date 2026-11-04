use super::*;

#[test]
fn tab_labels_keep_case_and_use_todo_fallbacks() {
    assert_eq!(tab_label(Path::new("TODO.md")), "TODO");
    assert_eq!(tab_label(Path::new(".md")), "todo");
    assert_eq!(tab_label(Path::new("")), "todo");
    assert_eq!(tab_label(Path::new("-notes.md")), "Todo -notes");
}

#[test]
fn agent_slugs_are_lowercase_collapsed_and_bounded() {
    let name = agent_name(
        Path::new("A very long résumé with spaces and punctuation.md"),
        0x12ab_cdef,
    );
    assert_eq!(name, "doc-a-very-long-r-sum-wi-abcdef");
    assert!(valid_agent_name(&name));
    assert!(name.len() <= 32);
    assert_eq!(agent_name(Path::new(".md"), 1), "doc-todo-000001");
}

#[test]
fn generated_names_keep_the_suffix_to_six_lowercase_hex_digits() {
    let name = agent_name(Path::new("notes.md"), u32::MAX);
    assert!(name.ends_with("ffffff"));
    assert!(valid_agent_name(&name));
    assert!(name.len() <= 32);
}

#[test]
fn arbitrary_names_follow_the_agent_rule() {
    assert!(valid_agent_name("a"));
    assert!(valid_agent_name("a0_b-c"));
    assert!(!valid_agent_name(""));
    assert!(!valid_agent_name("A-name"));
    assert!(!valid_agent_name("a.name"));
    assert!(!valid_agent_name(&"a".repeat(33)));
}
