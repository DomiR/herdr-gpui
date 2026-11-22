use super::*;
use std::ffi::OsStr;

fn after_session(command: &Command) -> Vec<&OsStr> {
    let args: Vec<&OsStr> = command.get_args().collect();
    assert_eq!(args[0], crate::setup_relay::SESSION);
    args[1..].to_vec()
}

#[test]
fn setup_prepares_path_then_runs_machine_add_without_interpolating_arguments() -> Result<()> {
    let request = Request::new("user@host", "Alice's $(printf INJECTED); device", "work")?;
    let command = setup_command(
        OsStr::new("/app/sheprd"),
        OsStr::new("/a path/herdr"),
        &request,
        false,
    );
    assert_eq!(command.get_program(), "/app/sheprd");
    let words = after_session(&command);
    assert_eq!(words[0], "/bin/sh");
    assert_eq!(words[1], "-c");
    assert_eq!(words[4], "no");
    assert_eq!(words[5], request.target());
    assert_eq!(words[8], "/a path/herdr");
    let arguments: Vec<_> = request.arguments().into_iter().map(OsStr::new).collect();
    assert_eq!(&words[9..], arguments);
    assert!(words[6].to_string_lossy().contains(".zshenv"));
    Ok(())
}

#[test]
fn key_setup_and_path_prepare_must_succeed_before_the_device_is_saved() -> Result<()> {
    let root = tempfile::tempdir()?;
    let log = root.path().join("log");
    for (name, script) in [
        (
            "ssh-copy-id",
            "#!/bin/sh\nprintf 'key\\n' >> \"$LOG\"\nexit \"$KEY_EXIT\"\n",
        ),
        (
            "ssh",
            "#!/bin/sh\nprintf 'path\\n' >> \"$LOG\"\nexit \"$SSH_EXIT\"\n",
        ),
        ("herdr", "#!/bin/sh\nprintf 'save\\n' >> \"$LOG\"\n"),
    ] {
        crate::test_executable::write(root.path().join(name), script, 0o755)?;
    }
    let request = Request::new("penso@box", "Mini 'n $HOME", "")?;
    let command = setup_command(
        OsStr::new("sheprd"),
        root.path().join("herdr").as_os_str(),
        &request,
        true,
    );
    let words = after_session(&command);
    for (key_exit, ssh_exit, expected) in [
        ("0", "0", "key\npath\nsave\n"),
        ("1", "0", "key\n"),
        ("0", "1", "key\npath\n"),
    ] {
        let _ = std::fs::remove_file(&log);
        let status = Command::new(words[0])
            .args(&words[1..])
            .env("PATH", root.path())
            .env("LOG", &log)
            .env("KEY_EXIT", key_exit)
            .env("SSH_EXIT", ssh_exit)
            .status()?;
        assert_eq!(status.success(), key_exit == "0" && ssh_exit == "0");
        assert_eq!(std::fs::read_to_string(&log)?, expected);
    }
    Ok(())
}

/// A setup that fails, as SSH refused by macOS does, used to leave its host
/// claimed for 15 minutes with nothing on screen, so adding it again said it
/// was "already being added". The relay now holds the claim and releases it
/// the moment the setup ends.
#[test]
fn a_failed_setup_releases_its_host_for_another_try() -> Result<()> {
    use crate::setup_relay::listen;
    use std::io::{Read, Write};
    let root = tempfile::tempdir()?;
    let claim = Claim::acquire(destination("failed-setup.test"), "default")?;
    let mut relay: Relay = listen(
        &root.path().join("gpui"),
        Command::new("/usr/bin/false"),
        Duration::from_secs(30),
    )?;
    relay.keep(claim);
    // The dialog is gone, but the setup still runs: the host stays claimed.
    assert!(matches!(
        Claim::acquire(destination("failed-setup.test"), "default"),
        Err(crate::Error::Devices(Error::Adding))
    ));
    let mut pane = std::os::unix::net::UnixStream::connect(relay.path())?;
    drop(relay);
    // Hello: 24 rows, 80 columns, no TERM.
    pane.write_all(&[1, 0, 0, 0, 4, 0, 24, 0, 80])?;
    // Read to the end of the session, which follows the setup's exit.
    pane.read_to_end(&mut Vec::new())?;
    let deadline = Instant::now() + Duration::from_secs(5);
    let retry = loop {
        match Claim::acquire(destination("failed-setup.test"), "default") {
            Err(crate::Error::Devices(Error::Adding)) if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(20));
            }
            result => break result,
        }
    };
    assert!(retry.is_ok());
    Ok(())
}

/// A relay nobody keeps, as when the workspace could not be created, takes
/// no claim: the dialog keeps it for a retry.
#[test]
fn an_unkept_relay_never_holds_the_host() -> Result<()> {
    let root = tempfile::tempdir()?;
    let relay: Relay = crate::setup_relay::listen(
        &root.path().join("gpui"),
        Command::new("/usr/bin/true"),
        Duration::from_secs(30),
    )?;
    let claim = Claim::acquire(destination("unkept.test"), "default")?;
    drop(relay);
    drop(claim);
    assert!(Claim::acquire(destination("unkept.test"), "default").is_ok());
    Ok(())
}
