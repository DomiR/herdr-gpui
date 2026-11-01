#![allow(clippy::unwrap_used)]
use super::*;

const DOC: &str = "# Work\n- [ ] write\n  - [x] outline\n- [x] read\n\n## Later\n- [ ] ship\n";

fn apply(text: &str, edit: Edit) -> String {
    Edit::apply_all(text, &[edit])
}

#[test]
fn lists_todos_with_their_sections() {
    let all = todos(DOC);
    let summary: Vec<(usize, bool, &str, usize, Option<&str>)> = all
        .iter()
        .map(|t| {
            (
                t.number,
                t.done,
                t.text.as_str(),
                t.depth,
                t.section.as_deref(),
            )
        })
        .collect();
    assert_eq!(
        summary,
        vec![
            (1, false, "write", 0, Some("Work")),
            (2, true, "outline", 2, Some("Work")),
            (3, true, "read", 0, Some("Work")),
            (4, false, "ship", 0, Some("Later")),
        ]
    );
}

#[test]
fn finds_by_number_or_unique_text() {
    assert_eq!(find(DOC, "4").unwrap().text, "ship");
    assert_eq!(find(DOC, "OUTL").unwrap().number, 2);
    assert!(matches!(find(DOC, "9"), Err(Error::NoTodo { .. })));
    assert!(matches!(find(DOC, "nothing"), Err(Error::NoTodo { .. })));
    assert!(matches!(
        find("- [ ] fix a\n- [ ] fix b\n", "fix"),
        Err(Error::AmbiguousTodo { count: 2, .. })
    ));
    assert_eq!(find("- [ ] fix\n- [ ] fix it\n", "fix").unwrap().number, 1);
}

#[test]
fn toggles_and_sets() {
    let checked = apply(DOC, toggle(DOC, 1).unwrap());
    assert!(checked.contains("- [x] write"));
    assert_eq!(set_done(DOC, 2, true), None);
    assert!(apply(DOC, set_done(DOC, 2, false).unwrap()).contains("- [ ] outline"));
    assert_eq!(toggle(DOC, 0), None);
}

#[test]
fn adds_after_the_last_task_or_under_a_heading() {
    let added = apply(DOC, add(DOC, "new", None).unwrap());
    assert!(added.ends_with("- [ ] ship\n- [ ] new\n"), "{added}");
    let under = apply(DOC, add(DOC, "next", Some("work")).unwrap());
    assert!(
        under.contains("- [x] read\n- [ ] next\n\n## Later"),
        "{under}"
    );
    let created = apply(DOC, add(DOC, "idea", Some("Ideas")).unwrap());
    assert!(
        created.ends_with("- [ ] ship\n\n## Ideas\n\n- [ ] idea\n"),
        "{created}"
    );
    assert_eq!(apply("", add("", "first", None).unwrap()), "- [ ] first\n");
    let empty_section = "# A\n\n# B\n";
    let placed = apply(empty_section, add(empty_section, "x", Some("A")).unwrap());
    assert_eq!(placed, "# A\n\n- [ ] x\n\n# B\n");
    assert!(matches!(add(DOC, "two\nlines", None), Err(Error::BadItem)));
    assert!(matches!(add(DOC, "  ", None), Err(Error::BadItem)));
}

#[test]
fn appends_a_block() {
    assert_eq!(
        apply("text", append("text", "more", None).unwrap()),
        "text\n\nmore\n"
    );
    assert_eq!(
        apply("- a\n", append("- a\n", "- b", None).unwrap()),
        "- a\n- b\n"
    );
}

#[test]
fn enter_continues_or_ends_a_list() {
    let text = "- [x] done";
    let (edit, caret) = enter(text, text.len()).unwrap();
    let next = apply(text, edit);
    assert_eq!(next, "- [x] done\n- [ ] ");
    assert_eq!(caret, next.len());
    let (edit, caret) = enter(&next, next.len()).unwrap();
    assert_eq!(apply(&next, edit), "- [x] done\n");
    assert_eq!(caret, "- [x] done\n".len());
    let numbered = "  3) step";
    let (edit, _) = enter(numbered, numbered.len()).unwrap();
    assert_eq!(apply(numbered, edit), "  3) step\n  4) ");
    // Splitting an item carries the rest into the new one.
    let (edit, _) = enter("- ab", 3).unwrap();
    assert_eq!(apply("- ab", edit), "- a\n- b");
    assert_eq!(enter("plain", 5), None);
    assert_eq!(enter("- [ ] x", 2), None);
}

#[test]
fn indents_items_with_their_children() {
    let text = "- a\n  - a1\n- b\n";
    let indented = Edit::apply_all(text, &indent(text, 0..1, false));
    assert_eq!(indented, "  - a\n    - a1\n- b\n");
    let back = Edit::apply_all(&indented, &indent(&indented, 0..1, true));
    assert_eq!(back, text);
    assert!(indent("para\n", 0..1, false).is_empty());
    assert_eq!(
        Edit::apply_all(text, &indent(text, 0..1, true)),
        "- a\n- a1\n- b\n"
    );
}

#[test]
fn moves_items_with_their_children() {
    let text = "- a\n  - a1\n- b\n- c\n";
    let (edit, line) = move_item(text, 2, true).unwrap();
    assert_eq!(apply(text, edit), "- b\n- a\n  - a1\n- c\n");
    assert_eq!(line, 0);
    let (edit, line) = move_item(text, 0, false).unwrap();
    assert_eq!(apply(text, edit), "- b\n- a\n  - a1\n- c\n");
    assert_eq!(line, 1);
    assert_eq!(move_item(text, 0, true), None);
    assert_eq!(move_item(text, 3, false), None);
    // A sub-item moves only among its siblings.
    assert_eq!(move_item(text, 1, true), None);
}

#[test]
fn counts_per_heading_and_done_lines() {
    let lines = markdown::lines(DOC);
    assert_eq!(section_counts(&lines), vec![(0, 2, 4), (5, 0, 1)]);
    let hidden = done_lines(&lines);
    assert_eq!(
        hidden
            .iter()
            .enumerate()
            .filter(|(_, h)| **h)
            .map(|(i, _)| i)
            .collect::<Vec<_>>(),
        vec![2, 3]
    );
}

#[test]
fn scoped_to_a_section() {
    let text = "# A\n- [ ] fix\nsame\n# B\n- [ ] fix\nsame\n";
    assert!(matches!(
        find(text, "fix"),
        Err(Error::AmbiguousTodo { .. })
    ));
    assert_eq!(find_in(text, "fix", Some("b")).unwrap().line, 4);
    // Numbers are the document's, as `todo list` shows them.
    assert_eq!(find_in(text, "2", Some("b")).unwrap().line, 4);
    assert!(matches!(
        find_in(text, "1", Some("b")),
        Err(Error::NoTodo { .. })
    ));
    let edit = replace(text, "same", "other", Some("b")).unwrap();
    assert_eq!(
        apply(text, edit),
        "# A\n- [ ] fix\nsame\n# B\n- [ ] fix\nother\n"
    );
    assert!(matches!(
        replace(text, "same", "x", None),
        Err(Error::AmbiguousText { count: 2 })
    ));
    let added = apply(text, add(text, "new", Some("a")).unwrap());
    assert!(
        added.starts_with("# A\n- [ ] fix\n- [ ] new\nsame"),
        "{added}"
    );
}

#[test]
fn appends_to_a_section() {
    let two = "# A\n- a\n\n# B\nb\n";
    assert_eq!(
        apply(two, append(two, "- a2", Some("a")).unwrap()),
        "# A\n- a\n- a2\n\n# B\nb\n"
    );
    assert!(matches!(append(two, " \n", None), Err(Error::EmptyText)));
    assert!(matches!(
        append(two, "x", Some("c")),
        Err(Error::NoSection { .. })
    ));
}

#[test]
fn appends_to_the_last_section_without_a_break() {
    let text = "# A\n- a";
    assert_eq!(
        apply(text, append(text, "- b", Some("a")).unwrap()),
        "# A\n- a\n- b\n"
    );
}
