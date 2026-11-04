use super::*;

fn depths(text: &str) -> Vec<(usize, usize)> {
    folds(&lines(text))
        .into_iter()
        .map(|fold| (fold.line, fold.depth))
        .collect()
}

#[test]
fn heading_depth_is_absolute_and_follows_nesting() {
    assert_eq!(
        depths("# A\ntext\n## B\ntext\n### C\ntext\n## D\ntext\n# E\ntext\n"),
        vec![(0, 1), (2, 2), (4, 3), (6, 2), (8, 1)]
    );
}

#[test]
fn a_document_starting_at_a_deeper_heading_keeps_absolute_depth() {
    assert_eq!(depths("## A\ntext\n### B\ntext\n"), vec![(0, 2), (2, 3)]);
}

#[test]
fn items_are_below_the_nearest_heading_and_each_enclosing_item() {
    assert_eq!(
        depths("## A\n- parent\n  - child\n    - grandchild\n## B\ntext\n"),
        vec![(0, 2), (1, 3), (2, 4), (4, 2)]
    );
}

#[test]
fn items_before_a_heading_start_at_depth_one() {
    assert_eq!(
        depths("- parent\n  - child\n    - grandchild\n# A\ntext\n"),
        vec![(0, 1), (1, 2), (3, 1)]
    );
}

#[test]
fn a_childless_item_has_no_fold() {
    assert!(
        !folds(&lines("- one\n# A\ntext\n"))
            .iter()
            .any(|fold| fold.line == 0)
    );
}
