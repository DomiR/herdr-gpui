//! A second daemon on the same state leaves the first one's control socket
//! alone.

use crate::control::{Request, Response};

#[tokio::test]
async fn a_second_daemon_does_not_take_a_live_control_socket() {
    let dir = tempfile::tempdir().unwrap();
    let state = crate::web::tests::state(dir.path());
    let (stop, _) = tokio::sync::watch::channel(false);
    assert!(!super::control::serving(dir.path()));
    super::control::serve(dir.path(), state.clone(), stop.clone()).unwrap();
    assert!(super::control::serving(dir.path()));
    let address = crate::control::address(dir.path());
    let status = || {
        tokio::task::spawn_blocking({
            let address = address.clone();
            move || crate::control::request(&address, &Request::Status)
        })
    };
    assert!(matches!(status().await.unwrap(), Ok(Response::Status(_))));

    let error = super::control::serve(dir.path(), state, stop).unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::AddrInUse);
    // The first daemon still answers.
    assert!(matches!(status().await.unwrap(), Ok(Response::Status(_))));
}

/// A daemon locked out by one already serving exits cleanly instead of
/// failing, so the service manager does not relaunch it every few seconds,
/// and it changes nothing the running one keeps (it does not even bind the
/// port, which a free one here would allow).
#[tokio::test(flavor = "multi_thread")]
async fn a_daemon_that_finds_another_serving_exits_cleanly() {
    let dir = tempfile::tempdir().unwrap();
    let state = crate::web::tests::state(dir.path());
    let (stop, _) = tokio::sync::watch::channel(false);
    super::control::serve(dir.path(), state, stop).unwrap();
    let state_dir = dir.path().to_str().unwrap();
    let options = super::serve(&["serve", "--state-dir", state_dir, "--port", "1"]);
    let ran = tokio::time::timeout(std::time::Duration::from_secs(10), super::run(options))
        .await
        .expect("returns at once");
    assert!(ran.is_ok(), "{ran:?}");
    let entries: Vec<_> = std::fs::read_dir(dir.path())
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect();
    assert!(
        !entries.iter().any(|name| name == "fleet"),
        "nothing the running daemon keeps was opened: {entries:?}"
    );
}
