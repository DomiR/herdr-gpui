use super::*;

fn editing(text: &str) -> Vec<Decoration> {
    editing_decorations(&lines(text)[0])
}

#[test]
fn editing_hides_only_a_bulleted_task_prefix() {
    assert_eq!(
        editing("- [ ] a"),
        vec![Decoration {
            range: 0..2,
            with: "",
        }]
    );
    assert_eq!(
        editing("  - [x] b"),
        vec![Decoration {
            range: 2..4,
            with: "",
        }]
    );
    assert_eq!(
        editing("* [ ] c"),
        vec![Decoration {
            range: 0..2,
            with: "",
        }]
    );
}

#[test]
fn editing_keeps_other_lines_raw() {
    for text in ["- a", "1. [ ] a", "# h", "> q", "**b**", "plain text"] {
        assert!(editing(text).is_empty(), "{text}");
    }
}
