use super::*;
use crate::config::preferences::Preference;

#[test]
fn confirm_close_tab_reads_booleans_and_names() -> anyhow::Result<()> {
    for (text, expected) in [
        ("confirm_close_tab = true", TabCloseConfirm::Busy),
        ("confirm_close_tab = false", TabCloseConfirm::Never),
        ("confirm_close_tab = 'busy'", TabCloseConfirm::Busy),
        ("confirm_close_tab = 'never'", TabCloseConfirm::Never),
        ("confirm_close_tab = 'always'", TabCloseConfirm::Always),
    ] {
        assert_eq!(Config::parse(text)?.confirm_close_tab, expected, "{text}");
    }
    for text in [
        "confirm_close_tab = 'sometimes'",
        "confirm_close_tab = 'Always'",
        "confirm_close_tab = 1",
    ] {
        assert!(Config::parse(text).is_err(), "{text}");
    }
    Ok(())
}

#[test]
fn confirm_close_tab_saves_booleans_older_builds_read() -> anyhow::Result<()> {
    let temp = TempDirectory::new()?;
    let path = temp.0.join("local.toml");
    for (choice, saved) in [
        (TabCloseConfirm::Always, "confirm_close_tab = \"always\""),
        (TabCloseConfirm::Never, "confirm_close_tab = false"),
        (TabCloseConfirm::Busy, "confirm_close_tab = true"),
    ] {
        Config::save_preference_path(Preference::ConfirmCloseTab(choice), &path)?;
        let text = fs::read_to_string(&path)?;
        assert!(text.contains(saved), "{text}");
        assert_eq!(Config::parse(&text)?.confirm_close_tab, choice);
    }
    Ok(())
}

#[test]
fn confirm_close_tab_labels_are_unique() {
    let mut labels: Vec<_> = TabCloseConfirm::ALL
        .iter()
        .map(|choice| choice.label())
        .collect();
    labels.sort_unstable();
    labels.dedup();
    assert_eq!(labels.len(), TabCloseConfirm::ALL.len());
}
