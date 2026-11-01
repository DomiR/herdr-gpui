use super::*;
use crate::config::{ChangeCounts, preferences::Preference};

#[test]
fn sidebar_numbers_are_on_unless_turned_off() -> anyhow::Result<()> {
    for config in [Config::default(), Config::parse(DEFAULT_CONFIG)?] {
        assert!(config.sidebar_style.numbers);
    }
    assert!(
        !Config::parse("[sidebar]\nnumbers = false")?
            .sidebar_style
            .numbers
    );
    assert!(
        Config::parse("[sidebar]\nnumbers = true")?
            .sidebar_style
            .numbers
    );
    for bad in ["\"off\"", "0"] {
        assert!(Config::parse(&format!("[sidebar]\nnumbers = {bad}")).is_err());
    }
    Ok(())
}

#[test]
fn turning_sidebar_numbers_off_writes_the_sidebar_key() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("local.toml");
    fs::write(&path, "[sidebar]\nchange_counts = \"local\" # mine\n")?;
    for shown in [false, true] {
        Config::save_preference_path(Preference::SidebarNumbers(shown), &path)?;
        let text = fs::read_to_string(&path)?;
        assert!(text.contains("change_counts = \"local\" # mine"), "{text}");
        let config = Config::parse(&text)?;
        assert_eq!(config.sidebar_style.numbers, shown);
        assert_eq!(config.sidebar_style.change_counts, ChangeCounts::Local);
    }
    Ok(())
}
