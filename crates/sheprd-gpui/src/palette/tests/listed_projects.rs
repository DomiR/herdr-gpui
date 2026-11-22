use super::*;
use std::sync::atomic::AtomicBool;

fn listed(path: &str, name: Option<&str>) -> ProjectConfig {
    ProjectConfig {
        path: path.into(),
        name: name.map(Into::into),
    }
}

#[test]
fn listed_projects_are_read_from_the_palette_table_and_bounded() -> anyhow::Result<()> {
    let config: PaletteConfig = toml::from_str(
        r#"projects = [{ path = "~/Notes/garden", name = "garden" }, { path = "$HOME/x" }]"#,
    )?;
    assert_eq!(
        config.projects,
        [
            listed("~/Notes/garden", Some("garden")),
            listed("$HOME/x", None)
        ]
    );
    config.validate()?;
    assert!(config.has_projects());
    assert!(!PaletteConfig::default().has_projects());
    // A field this version cannot honour is refused rather than ignored.
    assert!(
        toml::from_str::<PaletteConfig>(r#"projects = [{ path = "/x", host = "box" }]"#).is_err()
    );
    for projects in [
        vec![listed(" ", None)],
        vec![listed("/x", Some(""))],
        vec![listed("/x\n", None)],
        vec![listed("/x", None); 257],
    ] {
        let config = PaletteConfig {
            projects,
            ..PaletteConfig::default()
        };
        assert!(matches!(
            config.validate(),
            Err(crate::Error::Palette(Error::ListedProjects))
        ));
    }
    Ok(())
}

#[test]
fn listed_projects_are_named_and_join_the_roots_folders() -> anyhow::Result<()> {
    let garden = tempfile::tempdir()?;
    let root = tempfile::tempdir()?;
    std::fs::create_dir(root.path().join("alpha"))?;
    std::fs::create_dir(root.path().join("beta"))?;
    let text = |path: &std::path::Path| path.to_string_lossy().into_owned();
    let found = projects::collect(
        &[text(root.path())],
        &[
            listed(&text(garden.path()), Some("garden")),
            // Also inside the root: listed once, by its listed name.
            listed(&text(&root.path().join("alpha")), Some("Alpha project")),
            listed(&text(&root.path().join("missing")), None),
        ],
        &AtomicBool::new(false),
    );
    let labels: Vec<_> = found
        .projects
        .iter()
        .map(|project| project.label.as_str())
        .collect();
    assert_eq!(labels, ["Alpha project", "beta", "garden"]);
    assert_eq!(found.errors.len(), 1);
    for project in &found.projects {
        projects::validate(project)?;
    }
    Ok(())
}
