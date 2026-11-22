//! `push-status`: Sheprd on another computer asks this one, over SSH, how
//! its daemon sends iPhone pushes.

use super::{options::USAGE, *};
use crate::{
    apns::{Kind, Sent},
    app::push_status,
    control::push::{Mode, Outcome},
};

#[test]
fn push_status_takes_a_state_directory_alone() {
    assert!(matches!(
        parse(args(&["push-status"])).unwrap(),
        Invocation::PushStatus { state_dir: None }
    ));
    assert!(matches!(
        parse(args(&["push-status", "--state-dir", "/s"])).unwrap(),
        Invocation::PushStatus { state_dir: Some(dir) } if dir == std::path::Path::new("/s")
    ));
    assert!(parse(args(&["push-status", "--device", "d"])).is_err());
    assert!(USAGE.contains("push-status [--state-dir DIR]"));
}

#[test]
fn no_daemon_answers_nothing() {
    let dir = tempfile::tempdir().unwrap();
    assert_eq!(push_status::read(dir.path()), None);
}

#[tokio::test(flavor = "multi_thread")]
async fn the_running_daemon_answers_with_its_mode_and_last_pushes() {
    let dir = tempfile::tempdir().unwrap();
    let state = web::tests::state(dir.path());
    state.pushes.record(Kind::Alert, &Sent::Delivered, 20);
    let (stop, _) = watch::channel(false);
    control::serve(dir.path(), state, stop).unwrap();
    let path = dir.path().to_owned();
    let status = tokio::task::spawn_blocking(move || push_status::read(&path))
        .await
        .unwrap()
        .expect("the daemon answers");
    assert_eq!(status.mode, Mode::Off);
    assert_eq!(
        status.alert.map(|last| last.outcome),
        Some(Outcome::Delivered)
    );
}
