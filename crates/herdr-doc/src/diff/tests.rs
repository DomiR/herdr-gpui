#![allow(clippy::unwrap_used)]
use super::*;

fn round_trip(old: &str, new: &str) -> Vec<Edit> {
    let edits = edits(old, new);
    assert_eq!(Edit::apply_all(old, &edits), new, "{old:?} -> {new:?}");
    edits
}

#[test]
fn identical_texts_need_no_edits() {
    assert!(edits("a\nb\n", "a\nb\n").is_empty());
}

#[test]
fn a_changed_word_is_a_small_edit_inside_its_line() {
    let edits = round_trip("one\nthe quick fox\nthree\n", "one\nthe slow fox\nthree\n");
    assert!(edits.iter().all(|edit| edit.range.len() <= 5), "{edits:?}");
}

#[test]
fn lines_added_removed_and_unicode() {
    round_trip("a\nb\nc\n", "a\nc\nd\n");
    round_trip("", "new\n");
    round_trip("old\n", "");
    round_trip("héllo wörld 🎉\n", "hallo wörld 🎊!\n");
    round_trip("no newline", "no newline at end");
}

#[test]
fn edits_ascend_without_overlap() {
    let edits = round_trip("1\n2\n3\n4\n5\n", "0\n2\nx\n4\n6\n7\n");
    for pair in edits.windows(2) {
        assert!(pair[0].range.end <= pair[1].range.start, "{edits:?}");
    }
}

#[test]
fn offsets_map_through_edits() {
    let edits = vec![Edit::insert(2, "xx"), Edit::new(4..6, "y")];
    assert_eq!(Edit::map_offset(0, &edits), 0);
    assert_eq!(Edit::map_offset(2, &edits), 4);
    assert_eq!(Edit::map_offset(5, &edits), 7);
    assert_eq!(Edit::map_offset(8, &edits), 9);
}

#[test]
fn replacing_one_unique_place() {
    assert_eq!(
        Edit::replace_unique("a b a", "b", "c").unwrap(),
        Edit::new(2..3, "c")
    );
    assert!(matches!(
        Edit::replace_unique("a b a", "a", "c"),
        Err(crate::Error::AmbiguousText { count: 2 })
    ));
    assert!(matches!(
        Edit::replace_unique("a", "z", "c"),
        Err(crate::Error::NoText)
    ));
    assert!(matches!(
        Edit::replace_unique("a", "", "c"),
        Err(crate::Error::EmptyOld)
    ));
}
