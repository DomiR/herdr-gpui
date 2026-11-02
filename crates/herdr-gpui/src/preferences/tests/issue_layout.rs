//! The Linear section's list or board is remembered with the chrome.
use super::*;
use crate::orchestrator::IssueLayout;

#[core::prelude::v1::test]
fn the_issue_layout_round_trips_and_an_unknown_one_is_forgotten() {
    let directory = TestDirectory::new();
    let path = directory.0.join("preferences.json");
    let chrome = Chrome {
        issue_layout: Some(IssueLayout::Board),
        sidebar_width: Some(250.),
        ..Chrome::default()
    };
    write_chrome(&path, chrome.clone()).unwrap();
    assert_eq!(read_chrome(&path).unwrap(), chrome);
    // Files from before it, or with a value this build does not know, keep
    // the rest and fall back to the default list.
    for stored in [r#""grid""#, "3", "null"] {
        fs::write(
            &path,
            format!(r#"{{"sidebar_width_px": 250, "issue_layout": {stored}}}"#),
        )
        .unwrap();
        let read = read_chrome(&path).unwrap();
        assert_eq!(read.issue_layout, None, "{stored}");
        assert_eq!(read.sidebar_width, Some(250.), "{stored}");
    }
    fs::write(&path, r#"{"sidebar_width_px": 250}"#).unwrap();
    assert_eq!(read_chrome(&path).unwrap().issue_layout, None);
}
