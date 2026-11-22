//! A chat started with a brief (`sheprd chat new --prompt`): the program
//! is told its file through the tab's environment, and only a chat on this
//! machine takes one.

use super::*;
use crate::{
    chat::{
        brief::{Brief, FILE_ENV},
        start::{self, AgentChoice, command_line},
    },
    sidebar::layout_tests::fixture_window,
    window::MockPeer,
};
use serde_json::json;
use sheprd_chat_protocol::Agent;
use sheprd_client::protocol::{ClientMessage as DaemonMessage, ClientPaneInputEvent};

fn brief() -> Brief {
    Brief {
        file: PathBuf::from("/tmp/sheprd-brief-1.md"),
        from: Some(Agent::Claude),
    }
}

#[gpui::test]
fn a_brief_rides_in_the_tab_and_its_line(cx: &mut gpui::TestAppContext) {
    let mut peer = MockPeer::advertising(&["tab.create"]);
    let (view, cx) = cx.add_window_view(fixture_window);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            super::start::local_window(view, &peer, cx);
            view.chats.brief = Some(brief());
            view.start_chat(Agent::Codex, None, None).unwrap();
            view.chats.brief = None;
        })
    });
    let tab = peer.request();
    assert_eq!(
        tab["params"]["env"],
        json!({"HERDR_ACP": "/opt/herdr-acp", FILE_ENV: "/tmp/sheprd-brief-1.md"})
    );
    let created = json!({"result":{"type":"tab_created",
        "tab":{"tab_id":"w1:t9"},"root_pane":{"pane_id":"w1:p9"}}});
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            let id = tab["id"].as_str().unwrap().to_owned();
            view.live.script_response = Some((id, Some(Ok(created))));
            view.poll_chat_start(cx);
            assert!(view.flash.is_none(), "{:?}", view.flash);
        })
    });
    let typed = std::iter::repeat_with(|| peer.receive())
        .take(8)
        .find(|message| matches!(message, DaemonMessage::ClientShellPaneInput { .. }));
    let line = command_line(Agent::Codex, AgentChoice::Fixed, Some(&brief()));
    assert_eq!(
        typed.unwrap(),
        DaemonMessage::ClientShellPaneInput {
            pane_id: "w1:p9".into(),
            events: vec![
                ClientPaneInputEvent::TextCommit(line.clone()),
                crate::menu::enter_key(),
            ],
        }
    );
    // The line names the variable, never the file.
    assert!(!line.contains("sheprd-brief-1"), "{line}");
    if cfg!(unix) {
        assert_eq!(
            line,
            r#" sh -c 'exec "$HERDR_ACP" --brief-file "$SHEPRD_BRIEF_FILE" --brief-from claude codex'"#
        );
    }
}

#[gpui::test]
fn a_brief_does_not_start_a_chat_on_an_ssh_host(cx: &mut gpui::TestAppContext) {
    let peer = MockPeer::advertising(&["tab.create"]);
    let (view, cx) = cx.add_window_view(fixture_window);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            super::start::local_window(view, &peer, cx);
            view.endpoints[0].connection.target = sheprd_client::ConnectTarget::Ssh {
                target: "host".into(),
                session: "default".into(),
            };
            view.chats.brief = Some(brief());
            assert!(matches!(
                view.start_chat(Agent::Claude, None, None),
                Err(start::Error::BriefRemote)
            ));
            assert!(!view.chat_starting());
            // Without one it starts there as before.
            view.chats.brief = None;
            view.start_chat(Agent::Claude, None, None).unwrap();
        })
    });
}

#[test]
fn without_a_brief_the_line_is_unchanged() {
    if cfg!(unix) {
        assert_eq!(
            command_line(Agent::Codex, AgentChoice::Open, None),
            r#" sh -c 'exec "$HERDR_ACP" --choose codex'"#
        );
    }
}

/// An older `sheprd-acp` would refuse `--brief-file` and start nothing, so
/// a brief is handed only to a program whose help lists the option.
#[cfg(unix)]
#[test]
fn only_a_program_that_takes_briefs_is_given_one() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let program = |name: &str, help: &str| {
        let path = dir.path().join(name);
        std::fs::write(&path, format!("#!/bin/sh\necho '{help}'\n")).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        path
    };
    let current = program(
        "current",
        "  --brief-file PATH  Send PATH as the first prompt",
    );
    assert!(takes_briefs(&current));
    let older = program("older", "  --choose  Let clients switch agents");
    assert!(!takes_briefs(&older));
    assert!(!takes_briefs(std::path::Path::new(
        "/nonexistent/sheprd-acp"
    )));
}
