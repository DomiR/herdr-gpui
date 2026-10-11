use super::*;
use std::collections::HashSet;

#[test]
fn the_linear_filter_keeps_only_linear_issues() {
    let items = vec![
        item(Provider::Beads, "hg-1", "Bead", "2026-10-01T00:00:00Z"),
        item(
            Provider::Linear,
            "ENG-12",
            "Fix login",
            "2026-10-03T00:00:00Z",
        ),
        item(Provider::Github, "377", "Icons", "2026-10-04T00:00:00Z"),
        item(
            Provider::Linear,
            "ENG-7",
            "Session expiry",
            "2026-10-02T00:00:00Z",
        ),
    ];
    let runs = Runs::new(&[], &[], &[]);
    let linear = Filters {
        provider: Some(Provider::Linear),
        ..Filters::default()
    };
    let rows = issue_rows(&items, &runs, "", linear, Sort::Updated, &HashSet::new());
    let ids: Vec<&str> = rows
        .iter()
        .map(|row| items[row.item].identifier.as_str())
        .collect();
    assert_eq!(ids, ["ENG-12", "ENG-7"]);
}
