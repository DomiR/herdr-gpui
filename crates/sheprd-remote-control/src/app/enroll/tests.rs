use super::*;
use crate::control::DeviceId;
use sheprd_client::settings_files::{MANAGED_HEADER, OVERRIDES_SEED};

fn enable(path: &Path) -> crate::app::Result<RemoteConfig> {
    enable_settings_at(path, Duration::ZERO)
}

#[test]
fn the_input_is_the_enrollment_and_an_older_sheprds_apns_files_are_ignored() {
    let input: Input = serde_json::from_str(
        r#"{"name":"iPhone","public_key":"KEY","via":"MacBook Pro","replace":"d0"}"#,
    )
    .unwrap();
    assert_eq!(input.enroll.name, "iPhone");
    assert_eq!(input.enroll.replace, Some(DeviceId("d0".into())));
    let json = serde_json::to_value(&input).unwrap();
    assert!(json.get("apns").is_none());
    assert_eq!(serde_json::from_value::<Input>(json).unwrap(), input);

    // Sheprd before the push relay sent its APNs key along: still read,
    // and the key dropped, never kept or written.
    let older: Input = serde_json::from_str(
        r#"{"name":"iPhone","public_key":"KEY","via":"MacBook Pro","replace":"d0",
            "apns":{"config":"{\"team_id\":\"T\"}","key":"-----BEGIN PRIVATE KEY-----"}}"#,
    )
    .unwrap();
    assert_eq!(older, input);
    assert!(!format!("{older:?}").contains("PRIVATE KEY"));
}

#[test]
fn a_host_without_sheprd_settings_turns_them_on_for_a_later_install() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("herdr/config-gpui.toml");
    let config = enable(&path).unwrap();
    assert!(config.enabled);
    assert_eq!(config.port, crate::app::config::DEFAULT_PORT);
    // Sheprd installed here later reads its overrides, finds remote control
    // on, and keeps the daemon rather than removing its service. They hold
    // its first-launch seed too, which it no longer writes once they exist.
    let local = dir.path().join("herdr/config-gpui.local.toml");
    assert_eq!(
        fs::read_to_string(&local).unwrap(),
        format!("{OVERRIDES_SEED}\n[remote]\nenabled = true\n")
    );
    assert!(!path.exists());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(&local).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
    }
}

#[test]
fn a_managed_file_without_overrides_is_left_and_the_overrides_seeded() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config-gpui.toml");
    let managed = format!("{MANAGED_HEADER}\n# [remote]\n# enabled = false\n");
    fs::write(&path, &managed).unwrap();
    assert!(enable(&path).unwrap().enabled);
    assert_eq!(
        fs::read_to_string(dir.path().join("config-gpui.local.toml")).unwrap(),
        format!("{OVERRIDES_SEED}\n[remote]\nenabled = true\n")
    );
    assert_eq!(fs::read_to_string(&path).unwrap(), managed);
}

#[test]
fn current_sheprd_overrides_are_turned_on_not_its_managed_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config-gpui.toml");
    let managed = format!("{MANAGED_HEADER}\n# [remote]\n# enabled = false\n");
    fs::write(&path, &managed).unwrap();
    let local = dir.path().join("config-gpui.local.toml");
    fs::write(&local, "# mine\nlayout = \"compact\"\n").unwrap();
    assert!(enable(&path).unwrap().enabled);
    assert_eq!(
        fs::read_to_string(&local).unwrap(),
        "# mine\nlayout = \"compact\"\n\n[remote]\nenabled = true\n"
    );
    // Sheprd rewrites this file and never reads it.
    assert_eq!(fs::read_to_string(&path).unwrap(), managed);
}

#[test]
fn herdr_ui_settings_on_the_host_are_turned_on_and_kept() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config-gpui.toml");
    fs::write(
        &path,
        "# mine\ntheme = \"dark\"\n\n[remote]\nenabled = false\nport = 8443\n",
    )
    .unwrap();
    let config = enable(&path).unwrap();
    assert!(config.enabled);
    assert_eq!(config.port, 8443);
    let text = fs::read_to_string(&path).unwrap();
    assert_eq!(
        text,
        "# mine\ntheme = \"dark\"\n\n[remote]\nenabled = true\nport = 8443\n"
    );
    // Sheprd moves it into overrides itself, with the change.
    assert!(!dir.path().join("config-gpui.local.toml").exists());
    // Already on: the file is left as it is.
    fs::write(&path, "[remote]\nenabled = true # on\n").unwrap();
    assert!(enable(&path).unwrap().enabled);
    assert_eq!(
        fs::read_to_string(&path).unwrap(),
        "[remote]\nenabled = true # on\n"
    );
}

#[test]
fn a_move_sheprd_began_still_finishes() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config-gpui.toml");
    let local = dir.path().join("config-gpui.local.toml");
    // Sheprd copied the personal file into overrides and stopped before
    // replacing it; it finishes the move only while both match.
    let personal = "theme = \"dark\"\n";
    fs::write(&path, personal).unwrap();
    fs::write(&local, personal).unwrap();
    assert!(enable(&path).unwrap().enabled);
    // The copy is dropped, so Sheprd moves the changed file again.
    assert!(!local.exists());
    assert_eq!(
        fs::read_to_string(&path).unwrap(),
        "theme = \"dark\"\n\n[remote]\nenabled = true\n"
    );
}

#[test]
fn a_conflicting_personal_file_and_overrides_are_both_turned_on() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config-gpui.toml");
    let local = dir.path().join("config-gpui.local.toml");
    fs::write(&path, "theme = \"dark\"\n").unwrap();
    fs::write(&local, "theme = \"light\"\n").unwrap();
    assert!(enable(&path).unwrap().enabled);
    assert_eq!(
        fs::read_to_string(&path).unwrap(),
        "theme = \"dark\"\n\n[remote]\nenabled = true\n"
    );
    assert_eq!(
        fs::read_to_string(&local).unwrap(),
        "theme = \"light\"\n\n[remote]\nenabled = true\n"
    );
}

#[test]
fn broken_settings_are_reported_not_overwritten() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config-gpui.toml");
    fs::write(&path, "[remote\n").unwrap();
    assert!(matches!(
        enable(&path),
        Err(super::super::Error::Enroll(Error::Config { .. }))
    ));
    assert_eq!(fs::read_to_string(&path).unwrap(), "[remote\n");
    // Broken overrides stop the change before the personal file takes it.
    let local = dir.path().join("config-gpui.local.toml");
    fs::write(&path, "theme = \"dark\"\n").unwrap();
    fs::write(&local, "[remote\n").unwrap();
    assert!(matches!(
        enable(&path),
        Err(super::super::Error::Enroll(Error::Config { .. }))
    ));
    assert_eq!(fs::read_to_string(&local).unwrap(), "[remote\n");
    assert_eq!(fs::read_to_string(&path).unwrap(), "theme = \"dark\"\n");
}

mod settings_order;
