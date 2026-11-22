//! A continued conversation opens folded above its last prompt, and the
//! header shows and hides the turns before it.

use super::{
    overlay::{attach_with, open, poll},
    *,
};
use crate::config::ChatLayoutMode;
use gpui::{Modifiers, TestAppContext, px};
use sheprd_chat_protocol::{NoticeLevel, SessionInfo};
use sheprd_client::ConnectTarget;

fn user(id: u64, text: &str) -> Entry {
    Entry {
        id: EntryId(id),
        kind: EntryKind::User {
            files: 0,
            note: None,
            text: text.into(),
        },
    }
}

fn notice(id: u64, text: &str, details: Option<&str>) -> Entry {
    Entry {
        id: EntryId(id),
        kind: EntryKind::Notice {
            answers: Vec::new(),
            details: details.map(str::to_owned),
            level: NoticeLevel::Info,
            text: text.into(),
        },
    }
}

fn continued() -> ServerMessage {
    let entries = vec![
        user(1, "Make Moonframe the default"),
        notice(2, "Context compacted", Some("Summary of the earlier work")),
        user(3, "Carry on"),
        agent(4, "Done."),
        notice(5, "Continued from Claude's earlier session", None),
    ];
    let ServerMessage::Snapshot {
        cursor,
        mut transcript,
    } = snapshot(cursor(1, 0), entries)
    else {
        unreachable!("snapshot");
    };
    transcript.session = Some(SessionInfo {
        continued: Some(EntryId(5)),
        cwd: None,
        id: "s1".into(),
        title: None,
    });
    ServerMessage::Snapshot { cursor, transcript }
}

#[gpui::test]
fn earlier_turns_fold_above_the_last_prompt_until_shown(cx: &mut TestAppContext) {
    let (view, _peer, cx) = open(cx, ConnectTarget::Local);
    let program = Program::new();
    let _stream = attach_with(&view, &program, continued(), cx);
    assert!(cx.debug_bounds("chat-notice-2").is_none());
    let header = cx.debug_bounds("chat-earlier").unwrap();
    cx.simulate_click(header.center(), Modifiers::default());
    poll(&view, &program, cx);
    let shown = cx.debug_bounds("chat-notice-2").unwrap();
    let header = cx.debug_bounds("chat-earlier").unwrap();
    assert!(shown.origin.y < header.origin.y);
    cx.simulate_click(header.center(), Modifiers::default());
    poll(&view, &program, cx);
    assert!(cx.debug_bounds("chat-notice-2").is_none());
}

#[gpui::test]
fn the_fold_header_shows_in_every_layout(cx: &mut TestAppContext) {
    let (view, _peer, cx) = open(cx, ConnectTarget::Local);
    let program = Program::new();
    let _stream = attach_with(&view, &program, continued(), cx);
    for &mode in ChatLayoutMode::ALL {
        view.update(cx, |view, cx| {
            view.set_chat_layout_with(mode, |_| Ok(()), cx)
        });
        poll(&view, &program, cx);
        let header = cx.debug_bounds("chat-earlier");
        assert!(
            header.is_some_and(|header| header.size.height > px(0.)),
            "{mode:?}"
        );
        assert!(cx.debug_bounds("chat-notice-2").is_none(), "{mode:?}");
    }
}

#[gpui::test]
fn a_chat_that_continued_nothing_has_no_fold(cx: &mut TestAppContext) {
    let (view, _peer, cx) = open(cx, ConnectTarget::Local);
    let program = Program::new();
    let _stream = attach_with(
        &view,
        &program,
        snapshot(
            cursor(1, 0),
            vec![
                notice(1, "Context compacted", Some("Summary")),
                user(2, "Hello"),
            ],
        ),
        cx,
    );
    assert!(cx.debug_bounds("chat-earlier").is_none());
    assert!(cx.debug_bounds("chat-notice-1").is_some());
}
