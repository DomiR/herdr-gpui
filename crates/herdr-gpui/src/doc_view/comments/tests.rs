use super::prompt;

#[test]
fn the_prompt_fences_quotes_as_data() {
    let prompt = prompt(
        "/notes/todo.md",
        7,
        &[(3, "- [ ] ship ```it```".into(), "split this up".into())],
    );
    assert!(prompt.starts_with("Comments on /notes/todo.md I wrote in Herdr UI."));
    assert!(prompt.contains("version 7"));
    assert!(prompt.contains(
        "1. On line 3:\n   ````text\n   - [ ] ship ```it```\n   ````\n   Comment: split this up\n"
    ));
}
