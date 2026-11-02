use super::*;

#[test]
fn document_theme_defaults_when_table_or_key_is_absent() -> anyhow::Result<()> {
    for text in ["", "[documents]", "[documents]\ntheme = 'app'"] {
        assert_eq!(Config::parse(text)?.documents.theme, DocumentTheme::App);
    }
    assert_eq!(
        Config::parse(DEFAULT_CONFIG)?.documents.theme,
        DocumentTheme::App
    );
    assert_eq!(Config::default().documents.theme, DocumentTheme::App);
    Ok(())
}

#[test]
fn document_theme_parses_the_supported_values() -> anyhow::Result<()> {
    assert_eq!(
        Config::parse("[documents]\ntheme = 'app'")?.documents.theme,
        DocumentTheme::App
    );
    assert_eq!(
        Config::parse("[documents]\ntheme = 'plaintasks'")?
            .documents
            .theme,
        DocumentTheme::PlainTasks
    );
    assert_eq!(DocumentTheme::App.as_str(), "app");
    assert_eq!(DocumentTheme::PlainTasks.as_str(), "plaintasks");
    Ok(())
}

#[test]
fn document_theme_rejects_unknown_and_non_string_values() {
    for text in [
        "[documents]\ntheme = 'unknown'",
        "[documents]\ntheme = true",
        "[documents]\ntheme = 1",
    ] {
        assert!(
            matches!(Config::parse(text), Err(crate::Error::Toml(_))),
            "{text}"
        );
    }
}

#[test]
fn document_theme_edit_round_trip_uses_the_default_layer() -> anyhow::Result<()> {
    let temp = TempDirectory::new()?;
    let path = temp.0.join("config-gpui.toml");
    let local = path.with_extension("local.toml");
    let daemon = temp.0.join("absent.toml");
    fs::write(&local, "# Keep this comment\ntheme = 'Nord'\n")?;

    assert_eq!(
        Config::effective_value_at(&path, "documents.theme")?,
        Some((toml::Value::String("app".into()), Layer::Default))
    );
    Config::edit_key_at(
        &local,
        &daemon,
        "documents.theme",
        Some(parse_value("'plaintasks'")),
    )?;
    assert_eq!(
        Config::effective_value_at(&path, "documents.theme")?,
        Some((toml::Value::String("plaintasks".into()), Layer::Local,))
    );
    assert!(fs::read_to_string(&local)?.contains("# Keep this comment"));

    Config::edit_key_at(&local, &daemon, "documents.theme", None)?;
    assert_eq!(
        Config::check_files_at(&path, &daemon)?.documents.theme,
        DocumentTheme::App
    );
    assert_eq!(
        Config::effective_value_at(&path, "documents.theme")?,
        Some((toml::Value::String("app".into()), Layer::Default))
    );
    Ok(())
}
