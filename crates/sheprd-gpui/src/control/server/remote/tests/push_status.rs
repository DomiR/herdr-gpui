use super::*;
use crate::config::RemoteConfig;
use gpui::TestAppContext;
use sheprd_remote_control::control::{
    self as daemon,
    push::{Mode, PushStatus},
};

fn answering(_: &std::path::Path, request: &Request) -> Result<daemon::Response, daemon::Error> {
    match request {
        Request::PushStatus => Ok(daemon::Response::PushStatus(PushStatus {
            mode: Mode::PushRelay,
            ..PushStatus::default()
        })),
        _ => Ok(daemon::Response::Done),
    }
}

fn silent(_: &std::path::Path, _: &Request) -> Result<daemon::Response, daemon::Error> {
    Err(daemon::Error::NoAnswer)
}

fn reply(response: Response) -> Reply {
    match response {
        Response::Remote { reply } => reply,
        response => panic!("not a remote reply: {response:?}"),
    }
}

#[gpui::test]
async fn push_status_asks_the_daemon_only_while_remote_control_is_on(cx: &mut TestAppContext) {
    cx.update(|cx| Supervisor::fixture(cx, RemoteConfig::default(), answering));
    let off = cx
        .update(|cx| super::super::push_status::serve(false, true, cx))
        .err()
        .unwrap();
    assert_eq!(
        reply(off),
        Reply::PushStatus {
            enabled: false,
            push_relay: true,
            status: None,
            error: None,
            hosts: Vec::new(),
        }
    );
    let task = cx
        .update(|cx| super::super::push_status::serve(true, true, cx))
        .ok()
        .unwrap();
    let Reply::PushStatus {
        enabled,
        status,
        error,
        ..
    } = reply(task.await)
    else {
        panic!("not a push status");
    };
    assert!(enabled);
    assert_eq!(status.unwrap().mode, Mode::PushRelay);
    assert_eq!(error, None);
}

#[gpui::test]
async fn push_status_says_why_an_older_daemon_did_not_answer(cx: &mut TestAppContext) {
    cx.update(|cx| Supervisor::fixture(cx, RemoteConfig::default(), silent));
    let task = cx
        .update(|cx| super::super::push_status::serve(true, false, cx))
        .ok()
        .unwrap();
    let Reply::PushStatus { status, error, .. } = reply(task.await) else {
        panic!("not a push status");
    };
    assert_eq!(status, None);
    assert!(error.is_some());
}
