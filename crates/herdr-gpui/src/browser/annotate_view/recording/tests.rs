use super::{super::Annotations, Recording};
use crate::browser::TabId;
use std::time::Instant;

#[test]
fn the_record_button_says_what_the_recording_is_doing() {
    let now = Instant::now();
    let tab = TabId::test(1);
    assert_eq!(Recording::Starting { tab }.label(now), "Starting\u{2026}");
    assert_eq!(Recording::Building { tab }.label(now), "Building\u{2026}");
    assert_eq!(Recording::Building { tab }.tab(), tab);
}

#[test]
fn closing_a_page_drops_its_recording_whatever_it_is_doing() {
    let page = TabId::test(1);
    let other = TabId::test(2);
    for recording in [
        Recording::Starting { tab: page },
        Recording::Building { tab: page },
    ] {
        let mut annotations = Annotations {
            recording: Some(recording),
            ..Annotations::default()
        };
        // Another page closing leaves it alone.
        annotations.forget(other);
        assert!(annotations.recording.is_some());
        // Its own page closing ends it, so macOS's late answer is dropped
        // and a finished build finds no page to add a note to.
        annotations.forget(page);
        assert!(annotations.recording.is_none());
    }
}
