use super::*;
use std::{process::Command, time::Instant};

fn await_update(session: &Session, predicate: impl Fn(&Update) -> bool) -> Update {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(update) = session.poll()
            && predicate(&update)
        {
            return update;
        }
        assert!(Instant::now() < deadline, "setup did not update");
        thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn confirmations_are_explicit_and_fragmented_output_is_plain_text() {
    assert_eq!(prompt("Install? [Y/n]"), Prompt::Confirm);
    assert_eq!(prompt("Restart stops panes. [y/N]"), Prompt::Confirm);
    assert_eq!(prompt("Continue (yes/no/[fingerprint])?"), Prompt::Confirm);
    assert_eq!(prompt("Password:"), Prompt::Response);
    assert_eq!(prompt("Install? [Y/n]\nDownloading…"), Prompt::Response);
    let mut emulator = Emulator::new(TerminalSize::new(COLS, ROWS).unwrap(), 0).without_images();
    for bytes in [
        b"\x1b[31mInstall caf\xc3".as_slice(),
        b"\xa9?\x1b[0m [Y/n]",
        b"\x1b]52;c;c2VjcmV0\x07",
    ] {
        emulator.feed(bytes);
    }
    assert_eq!(plain_text(&emulator), "Install café? [Y/n]");
    let mut emulator = Emulator::new(TerminalSize::new(COLS, ROWS).unwrap(), 0).without_images();
    let long_prompt = format!("{} [y/N]", "x".repeat(98));
    emulator.feed(long_prompt.as_bytes());
    assert_eq!(plain_text(&emulator), long_prompt);
    assert_eq!(prompt(&plain_text(&emulator)), Prompt::Confirm);
}

#[test]
fn native_responses_are_bounded_and_do_not_wait_for_a_reader() {
    let (input, responses) = mpsc::sync_channel(1);
    let session = Session {
        input,
        cancelled: Arc::default(),
        state: Arc::default(),
    };
    assert!(!session.respond("yes\nrun something"));
    assert!(!session.respond(&"x".repeat(MAX_PAYLOAD)));
    assert!(session.respond("sécret"));
    assert!(!session.respond("another"));
    assert_eq!(responses.recv().unwrap().as_slice(), "sécret\n".as_bytes());
}

#[test]
fn closing_the_dialog_hangs_up_the_setup_and_releases_its_claim() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let (released, release) = mpsc::sync_channel(1);
    struct Held(SyncSender<()>);
    impl Drop for Held {
        fn drop(&mut self) {
            let _ = self.0.send(());
        }
    }
    let mut command = Command::new("/bin/sh");
    command.args(["-c", "printf 'Password:'; read -r response; printf 'done'"]);
    let relay = super::super::listen(dir.path(), command, Duration::from_secs(5))?;
    let session = Session::connect(relay, Held(released))?;
    await_update(&session, |update| update.output.contains("Password:"));
    drop(session);
    release.recv_timeout(Duration::from_secs(5)).unwrap();
    Ok(())
}

#[test]
fn responses_and_exit_status_flow_through_the_native_dialog() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let mut command = Command::new("/bin/sh");
    command.args(["-c", "stty -echo; printf 'Password:'; read -r response; printf '\\nConnected'; test \"$response\" = secret"]);
    let relay = super::super::listen(dir.path(), command, Duration::from_secs(5))?;
    let session = Session::connect(relay, ())?;
    await_update(&session, |update| update.output.contains("Password:"));
    assert!(session.respond("secret"));
    let update = await_update(&session, |update| update.result.is_some());
    assert!(!update.output.contains("secret"));
    assert!(update.output.contains("Connected"));
    assert_eq!(update.result.unwrap()?, 0);
    Ok(())
}

#[test]
fn responses_use_the_existing_relay_frame_format() {
    let (mut writer, mut reader) = UnixStream::pair().unwrap();
    write_response(&mut writer, b"yes\n").unwrap();
    assert_eq!(
        Frame::read(&mut reader).unwrap(),
        Some(Frame::Input(b"yes\n".to_vec()))
    );
    assert_eq!(
        write_response(&mut writer, &vec![b'x'; MAX_PAYLOAD + 1])
            .unwrap_err()
            .kind(),
        std::io::ErrorKind::InvalidInput,
    );
}
