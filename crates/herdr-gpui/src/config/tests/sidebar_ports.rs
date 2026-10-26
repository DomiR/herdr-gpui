use super::*;

#[test]
fn sidebar_ports_are_on_unless_turned_off() -> anyhow::Result<()> {
    for config in [Config::default(), Config::parse(DEFAULT_CONFIG)?] {
        assert!(config.sidebar_style.ports);
    }
    let off = Config::parse("[sidebar]\nports = false")?;
    assert!(!off.sidebar_style.ports);
    // The sidebar's key leaves the global switch, which also drives the
    // status bar and the scan, alone.
    assert!(off.show_listening_ports);
    assert!(off.unknown_keys.is_empty(), "{:?}", off.unknown_keys);
    assert!(Config::parse("[sidebar]\nports = \"off\"").is_err());
    let global = Config::parse("show_listening_ports = false")?;
    assert!(global.sidebar_style.ports);
    assert!(!global.show_listening_ports);
    Ok(())
}

#[test]
fn turning_sidebar_ports_off_writes_the_sidebar_key() -> anyhow::Result<()> {
    use crate::config::preferences::Preference;
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("local.toml");
    fs::write(&path, "[sidebar]\nfamily = 'Inter' # font\n")?;
    for shown in [false, true] {
        Config::save_preference_path(Preference::SidebarPorts(shown), &path)?;
        let text = fs::read_to_string(&path)?;
        assert!(text.contains("family = 'Inter' # font"), "{text}");
        let config = Config::parse(&text)?;
        assert_eq!(config.sidebar_style.ports, shown, "{text}");
        assert!(config.show_listening_ports, "{text}");
    }
    Ok(())
}
