use sheprd_chat_protocol::{Mode, Modes};

use super::*;

fn settings() -> Settings {
    Settings {
        modes: Some(Modes {
            available: vec![Mode {
                description: None,
                id: "plan".to_owned(),
                name: "Plan".to_owned(),
            }],
            current: "plan".to_owned(),
        }),
        options: Vec::new(),
    }
}

#[test]
fn saved_settings_load_back_per_agent() {
    let root = tempfile::tempdir().unwrap();
    let dir = root.path().join("settings");
    save(&dir, Agent::Codex, &settings()).unwrap();
    let known = load(&dir);
    assert_eq!(known.len(), 1);
    assert_eq!(known.get(&Agent::Codex), Some(&settings()));
    // Saving nothing forgets the agent's, and forgetting twice is fine.
    save(&dir, Agent::Codex, &Settings::default()).unwrap();
    assert!(!path(&dir, Agent::Codex).exists());
    save(&dir, Agent::Codex, &Settings::default()).unwrap();
    assert!(load(&dir).is_empty());
}

#[cfg(unix)]
#[test]
fn the_files_are_private() {
    use std::os::unix::fs::PermissionsExt;
    let root = tempfile::tempdir().unwrap();
    let dir = root.path().join("settings");
    save(&dir, Agent::Claude, &settings()).unwrap();
    let mode = |path: &Path| fs::metadata(path).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode(&dir), 0o700);
    assert_eq!(mode(&path(&dir, Agent::Claude)), 0o600);
}

#[test]
fn a_damaged_or_oversized_file_is_not_known() {
    let root = tempfile::tempdir().unwrap();
    let dir = root.path();
    fs::write(path(dir, Agent::Claude), b"{not json").unwrap();
    fs::write(
        path(dir, Agent::Codex),
        vec![b' '; usize::try_from(MAX_FILE).unwrap() + 1],
    )
    .unwrap();
    assert!(load(dir).is_empty());
}
