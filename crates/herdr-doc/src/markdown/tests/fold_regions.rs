use super::*;

fn regions(text: &str) -> Vec<(usize, Range<usize>, FoldKind)> {
    folds(&lines(text))
        .into_iter()
        .map(|fold| (fold.line, fold.hidden, fold.kind))
        .collect()
}

#[test]
fn a_heading_folds_its_section_and_nested_sections() {
    let text = "# A\ntext\n## B\n- x\n\n# C\nmore\n";
    assert_eq!(
        regions(text),
        vec![
            (0, 1..4, FoldKind::Section),
            (2, 3..4, FoldKind::Section),
            (5, 6..7, FoldKind::Section),
        ]
    );
}

#[test]
fn an_item_folds_its_sub_items() {
    let text = "- [ ] a\n  - [ ] a1\n    - a1x\n  - [x] a2\n- [ ] b\n- c\n";
    assert_eq!(
        regions(text),
        vec![(0, 1..4, FoldKind::Item), (1, 2..3, FoldKind::Item)]
    );
}

#[test]
fn a_code_block_folds_to_its_first_line() {
    let text = "```\na\nb\n```\n```\n```\n";
    assert_eq!(regions(text), vec![(0, 1..4, FoldKind::Code)]);
}

#[test]
fn an_empty_section_does_not_fold() {
    assert!(regions("# A\n\n# B\n").is_empty());
}
