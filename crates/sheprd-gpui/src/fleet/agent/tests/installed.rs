//! Every command Sheprd runs through the agent goes to the one it
//! installed, which its service runs, whatever else is on `PATH`. Unix
//! only: they run the script under `sh`.

use super::*;

#[test]
fn commands_go_to_the_installed_agent_not_one_earlier_on_path() {
    use std::os::unix::fs::PermissionsExt as _;
    let dir = tempfile::tempdir().unwrap();
    let fake = |path: &Path, says: &str| {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, format!("#!/bin/sh\necho {says} \"$@\"\n")).unwrap();
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
    };
    let home = dir.path().join("home");
    let old = dir.path().join("usr/bin");
    fake(&old.join(PROGRAM), "old");
    fake(&home.join(".local/bin").join(PROGRAM), "installed");
    let script = RUN.replace("ARGS", "fleet apply");
    #[allow(clippy::disallowed_methods)] // A local `sh`, in a test.
    let output = std::process::Command::new("sh")
        .args(["-c", &script])
        .env_clear()
        .env("HOME", &home)
        .env("PATH", format!("{}:/usr/bin:/bin", old.display()))
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "installed fleet apply\n"
    );
}

#[test]
fn without_the_installed_agent_nothing_runs() {
    let dir = tempfile::tempdir().unwrap();
    let script = RUN.replace("ARGS", "status --json");
    #[allow(clippy::disallowed_methods)] // A local `sh`, in a test.
    let output = std::process::Command::new("sh")
        .args(["-c", &script])
        .env_clear()
        .env("HOME", dir.path())
        .env("PATH", "/usr/bin:/bin")
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(output.stdout.is_empty());
    assert_eq!(parse_status(&output.stdout).unwrap(), None);
}
