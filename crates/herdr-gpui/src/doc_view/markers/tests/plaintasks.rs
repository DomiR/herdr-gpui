use super::*;

#[test]
fn marker_styles_use_their_expected_side_ratios() {
    let start = point(px(20.), px(12.));
    let end = point(px(80.), start.y);
    let height = px(30.);
    let checkbox = slot(start, end, height, 0.62);
    let plaintasks = slot(start, end, height, 0.36);
    assert_eq!(checkbox.size.width, px(18.6));
    assert_eq!(plaintasks.size.width, px(10.8));
}
