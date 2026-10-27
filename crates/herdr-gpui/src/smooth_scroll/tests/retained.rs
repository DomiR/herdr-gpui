use super::*;

#[test]
fn reversing_a_settled_gesture_draws_cached_rows_before_the_remote_reply() {
    let mut scroll = SmoothScroll::default();
    let now = Instant::now();
    assert_eq!(scroll.wheel(&back(10), "pane", 2., now), Some(2));
    scroll.observe(&back(10), &back(12));
    assert!(scroll.slide(now + SPREAD).is_none());
    assert_eq!(scroll.offset(), None);
    let later = now + 100 * SPREAD;
    assert_eq!(scroll.wheel(&back(12), "pane", -1., later), Some(-1));
    // No observe/response: the cached screen supplies the newly exposed row.
    assert_eq!(offset(&mut scroll, later + SPREAD), Some((-1., vec![2])));
    assert_eq!(scroll.offset(), Some((back(12).panes[0].inner_rect, -1.)));
    scroll.observe(&back(12), &back(11));
    assert!(scroll.slide(later + 2 * SPREAD).is_none());
}

#[test]
fn retained_rows_still_stop_at_the_cache_edge_and_clear_on_reset() {
    let mut scroll = SmoothScroll::default();
    let now = Instant::now();
    scroll.wheel(&back(10), "pane", 2., now);
    scroll.observe(&back(10), &back(12));
    scroll.slide(now + SPREAD);
    scroll.wheel(&back(12), "pane", -20., now + SPREAD);
    assert_eq!(offset(&mut scroll, now + 2 * SPREAD), Some((-2., vec![2])));
    scroll.clear();
    scroll.wheel(&back(12), "pane", -1., now + 3 * SPREAD);
    assert_eq!(offset(&mut scroll, now + 4 * SPREAD), Some((0., vec![])));
}
