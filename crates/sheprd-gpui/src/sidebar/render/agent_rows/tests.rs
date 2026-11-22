use super::*;
use core::prelude::v1::test;

fn marks(rows: &[bool]) -> Vec<Option<Mark>> {
    let mut favorites = Favorites::default();
    rows.iter()
        .map(|favorite| favorites.next(*favorite))
        .collect()
}

#[test]
fn favorites_get_a_heading_above_and_a_rule_below() {
    assert_eq!(
        marks(&[true, true, false, false]),
        [Some(Mark::Heading), None, Some(Mark::Rule), None]
    );
}

#[test]
fn only_favorites_have_no_rule_and_none_have_no_heading() {
    assert_eq!(marks(&[true, true]), [Some(Mark::Heading), None]);
    assert_eq!(marks(&[false, false]), [None, None]);
    assert!(marks(&[]).is_empty());
}

#[test]
fn each_mark_counts_as_a_row_once() {
    let mut favorites = Favorites::default();
    favorites.next(true);
    assert_eq!(favorites.take_marks(), 1);
    favorites.next(true);
    assert_eq!(favorites.take_marks(), 0);
    favorites.next(false);
    assert_eq!(favorites.take_marks(), 1);
}
