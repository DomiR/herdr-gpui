use super::*;

#[test]
fn unwrapped_slot_matches_the_legacy_square() {
    for (x, width, height) in [(40., 24., 24.), (80., 5., 12.8), (120., 48., 48.)] {
        let start = point(px(x), px(32.));
        let end = point(px(x + width), start.y);
        let height = px(height);
        let side = (height * 0.62).min((end.x - start.x).max(px(10.)));
        let center = point((start.x + end.x) / 2., start.y + height / 2.);
        assert_eq!(
            slot(start, end, height),
            Bounds::centered_at(center, size(side, side))
        );
    }
}

#[test]
fn wrapped_slot_stays_finite_on_the_start_row() {
    let start = point(px(240.), px(32.));
    let height = px(24.);
    let square = slot(start, point(px(4.), px(56.)), height);
    assert_eq!(square, slot(start, point(px(250.), start.y), height));
    assert_eq!(square.left(), start.x);
    assert_eq!(square.size.width, px(10.));
    assert!(square.top() >= start.y);
    assert!(square.bottom() <= start.y + height);
    for value in [square.left(), square.top(), square.right(), square.bottom()] {
        assert!(f32::from(value).is_finite());
    }
}
