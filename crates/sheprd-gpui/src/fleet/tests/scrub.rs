//! What `fleet scrub` on a host printed: success only when it says its
//! access is gone.

use super::*;

#[test]
fn a_scrub_succeeds_only_when_access_is_known_gone() {
    assert!(remote::scrub_outcome(br#"{"scrubbed":true,"failed":false}"#).is_ok());
    assert!(matches!(
        remote::scrub_outcome(br#"{"scrubbed":true,"failed":true}"#),
        Err(Error::Unscrubbed)
    ));
    assert!(matches!(
        remote::scrub_outcome(br#"{"scrubbed":false,"failed":true}"#),
        Err(Error::Unscrubbed)
    ));
    assert!(matches!(
        remote::scrub_outcome(br#"{"scrubbed":false,"failed":false}"#),
        Err(Error::NoProgram)
    ));
    // An answer that does not say whether anything failed is not trusted.
    assert!(matches!(
        remote::scrub_outcome(br#"{"scrubbed":true}"#),
        Err(Error::Answer(_))
    ));
}

#[test]
fn the_relay_agent_is_scrubbed_before_the_daemon() {
    // Forgetting the daemon's fleet may end the session the script runs
    // in: the agent's cleanup must already have run.
    let agent = remote::SCRUB.find("\"$agent\" fleet scrub").unwrap();
    let daemon = remote::SCRUB.find("\"$program\" fleet scrub").unwrap();
    assert!(agent < daemon);
}

/// Runs the scrub script under `sh` with `home` as `HOME` and `path` as
/// `PATH`, and returns what it printed.
#[cfg(unix)]
fn run_scrub(home: &std::path::Path, path: &std::ffi::OsStr) -> String {
    use std::io::Write as _;
    let mut child = std::process::Command::new("/bin/sh")
        .arg("-c")
        .arg(remote::SCRUB)
        .env_clear()
        .env("HOME", home)
        .env("PATH", path)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(b"").unwrap();
    let output = child.wait_with_output().unwrap();
    String::from_utf8(output.stdout).unwrap()
}

/// An executable `sheprd-relay-agent` in `dir` that knows `fleet scrub`
/// and leaves `marker` when it runs it.
#[cfg(unix)]
fn agent(dir: &std::path::Path, marker: &std::path::Path) {
    use std::os::unix::fs::PermissionsExt as _;
    std::fs::create_dir_all(dir).unwrap();
    let path = dir.join("sheprd-relay-agent");
    std::fs::write(
        &path,
        format!(
            "#!/bin/sh\ncase \"$1\" in --help) echo 'usage: fleet scrub';; fleet) : > '{}';; esac\n",
            marker.display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
}

#[cfg(unix)]
#[test]
fn the_scrub_runs_the_installed_agent_never_another_on_path() {
    let root = tempfile::tempdir().unwrap();
    let home = root.path().join("home");
    let decoy = root.path().join("decoy");
    let (installed_ran, decoy_ran) = (root.path().join("installed"), root.path().join("decoy-ran"));
    agent(&decoy, &decoy_ran);
    // The decoy comes first on PATH; the system's tools follow.
    let path =
        std::env::join_paths([decoy.as_path(), "/usr/bin".as_ref(), "/bin".as_ref()]).unwrap();
    // With no agent installed, the step is skipped, decoy or not.
    std::fs::create_dir_all(&home).unwrap();
    let printed = run_scrub(&home, &path);
    assert!(!decoy_ran.exists(), "a copy on PATH must never run");
    assert!(remote::scrub_outcome(printed.as_bytes()).is_err());
    // Installed where Sheprd puts it, that copy is the one run.
    agent(&home.join(".local/bin"), &installed_ran);
    let printed = run_scrub(&home, &path);
    assert!(installed_ran.exists());
    assert!(!decoy_ran.exists(), "a copy on PATH must never run");
    assert!(remote::scrub_outcome(printed.as_bytes()).is_ok());
}

/// An executable daemon program `name` in `dir` that knows `fleet scrub`,
/// adds a line to `marker` each time it runs it, and succeeds only when
/// `succeeds`.
#[cfg(unix)]
fn daemon(dir: &std::path::Path, name: &str, marker: &std::path::Path, succeeds: bool) {
    use std::os::unix::fs::PermissionsExt as _;
    std::fs::create_dir_all(dir).unwrap();
    let path = dir.join(name);
    let status = if succeeds { 0 } else { 1 };
    std::fs::write(
        &path,
        format!(
            "#!/bin/sh\ncase \"$1\" in --help) echo 'usage: fleet scrub';; fleet) echo ran >> '{}'; exit {status};; esac\n",
            marker.display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
}

#[cfg(unix)]
#[test]
fn every_daemon_program_that_may_hold_the_fleet_is_scrubbed() {
    for (installed_ok, decoy_ok) in [(true, true), (false, true), (true, false)] {
        let root = tempfile::tempdir().unwrap();
        let home = root.path().join("home");
        let decoy = root.path().join("decoy");
        let (installed_ran, decoy_ran) =
            (root.path().join("installed"), root.path().join("decoy-ran"));
        daemon(
            &home.join(".local/bin"),
            "sheprd-remote-control",
            &installed_ran,
            installed_ok,
        );
        daemon(&decoy, "herdr-remote-control", &decoy_ran, decoy_ok);
        let path =
            std::env::join_paths([decoy.as_path(), "/usr/bin".as_ref(), "/bin".as_ref()]).unwrap();
        let printed = run_scrub(&home, &path);
        // Both copies were scrubbed, whichever failed.
        assert!(installed_ran.exists() && decoy_ran.exists());
        assert_eq!(
            remote::scrub_outcome(printed.as_bytes()).is_ok(),
            installed_ok && decoy_ok,
            "{printed}"
        );
    }
}

#[cfg(unix)]
#[test]
fn a_daemon_program_reached_twice_is_scrubbed_once() {
    use std::os::unix::fs::symlink;
    let root = tempfile::tempdir().unwrap();
    let home = root.path().join("home");
    let bin = home.join(".local/bin");
    let ran = root.path().join("ran");
    daemon(&bin, "sheprd-remote-control", &ran, true);
    // The same program again on PATH, through a link.
    let linked = root.path().join("linked");
    std::fs::create_dir_all(&linked).unwrap();
    symlink(
        bin.join("sheprd-remote-control"),
        linked.join("sheprd-remote-control"),
    )
    .unwrap();
    let path =
        std::env::join_paths([linked.as_path(), "/usr/bin".as_ref(), "/bin".as_ref()]).unwrap();
    let printed = run_scrub(&home, &path);
    assert_eq!(std::fs::read_to_string(&ran).unwrap().lines().count(), 1);
    assert!(
        remote::scrub_outcome(printed.as_bytes()).is_ok(),
        "{printed}"
    );
}

#[cfg(unix)]
#[test]
fn a_daemon_program_without_fleet_scrub_is_left_alone() {
    use std::os::unix::fs::PermissionsExt as _;
    let root = tempfile::tempdir().unwrap();
    let home = root.path().join("home");
    let bin = home.join(".local/bin");
    std::fs::create_dir_all(&bin).unwrap();
    let ran = root.path().join("ran");
    // An older program, from before the fleet, holds no fleet state.
    let path = bin.join("sheprd-remote-control");
    std::fs::write(
        &path,
        format!(
            "#!/bin/sh\ncase \"$1\" in --help) echo 'usage: serve';; *) : > '{}';; esac\n",
            ran.display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    let printed = run_scrub(&home, std::ffi::OsStr::new("/usr/bin:/bin"));
    assert!(!ran.exists());
    // Nothing that held a fleet ran: not reported as access gone.
    assert!(remote::scrub_outcome(printed.as_bytes()).is_err());
}

/// An executable `name` in `dir` whose usage is `usage` and that leaves
/// `marker` when asked to `fleet scrub`.
#[cfg(unix)]
fn program(dir: &std::path::Path, name: &str, usage: &str, marker: &std::path::Path) {
    use std::os::unix::fs::PermissionsExt as _;
    std::fs::create_dir_all(dir).unwrap();
    let path = dir.join(name);
    std::fs::write(
        &path,
        format!(
            "#!/bin/sh\ncase \"$1\" in --help) echo '{usage}';; fleet) : > '{}';; esac\n",
            marker.display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
}

#[cfg(unix)]
#[test]
fn a_program_that_knows_the_fleet_but_not_scrub_fails_the_scrub() {
    let path = std::ffi::OsStr::new("/usr/bin:/bin");
    // An older daemon that already kept a fleet, beside an agent that
    // scrubs: the daemon may still let members in, so this is no success.
    let root = tempfile::tempdir().unwrap();
    let (home, ran) = (root.path().join("home"), root.path().join("ran"));
    let bin = home.join(".local/bin");
    program(&bin, "sheprd-relay-agent", "usage: fleet scrub", &ran);
    program(
        &bin,
        "sheprd-remote-control",
        "usage: fleet apply | fleet bundle",
        &ran,
    );
    let printed = run_scrub(&home, path);
    assert!(matches!(
        remote::scrub_outcome(printed.as_bytes()),
        Err(Error::Unscrubbed)
    ));
    // The same for an older agent beside a daemon that scrubs.
    let root = tempfile::tempdir().unwrap();
    let (home, ran) = (root.path().join("home"), root.path().join("ran"));
    let bin = home.join(".local/bin");
    program(&bin, "sheprd-relay-agent", "usage: fleet apply", &ran);
    program(&bin, "sheprd-remote-control", "usage: fleet scrub", &ran);
    let printed = run_scrub(&home, path);
    assert!(matches!(
        remote::scrub_outcome(printed.as_bytes()),
        Err(Error::Unscrubbed)
    ));
}
