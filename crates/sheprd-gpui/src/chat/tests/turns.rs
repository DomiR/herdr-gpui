//! The turns a host's layout draws in a chat's left gutter: where the gutter
//! is as the chat lays out, the turn being read, a tick's click scrolling
//! back to its prompt, and nothing once the chat leaves the screen.

use super::{
    overlay::{attach_with, open},
    *,
};
use crate::{SheprdWindow, sidebar::hosted::Action};
use gpui::{Entity, TestAppContext, VisualTestContext};
use serde_json::Value;
use sheprd_client::ConnectTarget;

const PANE: &str = "w1:p1";

fn user(id: u64, text: &str) -> Entry {
    Entry {
        id: EntryId(id),
        kind: EntryKind::User {
            text: text.into(),
            files: 0,
            note: None,
        },
    }
}

fn redraw(view: &Entity<SheprdWindow>, cx: &mut VisualTestContext) {
    view.update(cx, |_, cx| cx.notify());
    cx.run_until_parked();
}

fn turns(view: &Entity<SheprdWindow>, cx: &mut VisualTestContext) -> Value {
    view.read_with(cx, |view, _| view.hosted_chat_turns())
}

/// Three prompts, each with a reply long enough to scroll.
fn opened(
    view: &Entity<SheprdWindow>,
    program: &Program,
    hosted: bool,
    cx: &mut VisualTestContext,
) -> Peer {
    view.update(cx, |view, _| {
        view.hosted = hosted;
        view.config.chat.max_width = 400;
    });
    let mut entries = Vec::new();
    for (turn, prompt) in ["Why does it flicker?", "Fix it", "Ship it"]
        .iter()
        .enumerate()
    {
        let base = turn as u64 * 20 + 1;
        entries.push(user(base, prompt));
        entries.push(agent(base + 1, &format!("Reply {turn}: the start of it.")));
        entries.extend((base + 2..base + 20).map(|id| agent(id, "More of the reply.")));
    }
    let stream = attach_with(view, program, snapshot(cursor(1, 0), entries), cx);
    redraw(view, cx);
    redraw(view, cx);
    stream
}

#[gpui::test]
fn a_hosted_chat_reports_its_gutter_and_turns(cx: &mut TestAppContext) {
    let (view, _peer, cx) = open(cx, ConnectTarget::Local);
    let program = Program::new();
    let _stream = opened(&view, &program, true, cx);
    let turns = turns(&view, cx);
    assert_eq!(turns["pane"], PANE);
    let labels: Vec<&str> = turns["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| entry["label"].as_str().unwrap())
        .collect();
    assert_eq!(labels, ["Why does it flicker?", "Fix it", "Ship it"]);
    assert_eq!(turns["entries"][1]["summary"], "Reply 1: the start of it.");
    // At the end and following: the last turn is the one read.
    assert_eq!(turns["selected"], "41");
    // The gutter left of the 400-pixel column, inside the chat.
    let chat = cx.debug_bounds("chat-view").unwrap();
    let width = turns["width"].as_f64().unwrap() as f32;
    let expected = ((f32::from(chat.size.width) - 400.) / 2.).floor();
    assert!((width - expected).abs() <= 1., "{width} vs {expected}");
    let left = turns["left"].as_f64().unwrap() as f32;
    assert!(left >= f32::from(chat.origin.x) - 1., "{left} in {chat:?}");
    // Three ticks, stacked tightly in the middle of the entries, which
    // leave the header above and the composer below.
    assert_eq!(turns["height"], 96.);
    let top = turns["top"].as_f64().unwrap() as f32;
    assert!(top > f32::from(chat.origin.y) + 100., "{top} in {chat:?}");
    assert!(
        top + 96. < f32::from(chat.bottom()) - 100.,
        "{top} in {chat:?}"
    );
}

#[gpui::test]
fn a_tick_scrolls_back_to_its_prompt_and_marks_it(cx: &mut TestAppContext) {
    let (view, _peer, cx) = open(cx, ConnectTarget::Local);
    let program = Program::new();
    let _stream = opened(&view, &program, true, cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.hosted_action(
                Action::ChatTurn {
                    pane: PANE.into(),
                    entry: 1,
                },
                window,
                cx,
            );
        });
    });
    redraw(&view, cx);
    let top = view.read_with(cx, |view, _| {
        view.chats
            .pane(PANE)
            .unwrap()
            .list
            .logical_scroll_top()
            .item_ix
    });
    assert_eq!(top, 0, "the first prompt is at the top");
    assert_eq!(turns(&view, cx)["selected"], "1");
}

#[gpui::test]
fn an_unknown_tick_changes_nothing(cx: &mut TestAppContext) {
    let (view, _peer, cx) = open(cx, ConnectTarget::Local);
    let program = Program::new();
    let _stream = opened(&view, &program, true, cx);
    let before = turns(&view, cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.hosted_action(
                Action::ChatTurn {
                    pane: PANE.into(),
                    entry: 999,
                },
                window,
                cx,
            );
        });
    });
    redraw(&view, cx);
    assert_eq!(turns(&view, cx), before);
}

#[gpui::test]
fn the_window_s_own_layout_reports_nothing(cx: &mut TestAppContext) {
    let (view, _peer, cx) = open(cx, ConnectTarget::Local);
    let program = Program::new();
    let _stream = opened(&view, &program, false, cx);
    assert_eq!(turns(&view, cx), Value::Null);
}

#[gpui::test]
fn the_turns_go_with_the_chat(cx: &mut TestAppContext) {
    let (view, _peer, cx) = open(cx, ConnectTarget::Local);
    let program = Program::new();
    let _stream = opened(&view, &program, true, cx);
    assert!(!turns(&view, cx).is_null());
    let stamp = view.read_with(cx, |view, _| view.hosted_stamp());
    // Back to its terminal: the chat is no longer drawn.
    view.update(cx, |view, _| {
        view.chats.set_raw(PANE, true);
    });
    redraw(&view, cx);
    assert_eq!(turns(&view, cx), Value::Null);
    assert!(view.read_with(cx, |view, _| view.hosted_stamp()) != stamp);
}

#[gpui::test]
fn a_column_without_a_limit_leaves_no_gutter(cx: &mut TestAppContext) {
    let (view, _peer, cx) = open(cx, ConnectTarget::Local);
    let program = Program::new();
    let _stream = opened(&view, &program, true, cx);
    view.update(cx, |view, _| view.config.chat.max_width = 0);
    redraw(&view, cx);
    assert_eq!(turns(&view, cx), Value::Null);
}
