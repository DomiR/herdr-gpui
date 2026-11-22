use super::*;
use gpui::{Modifiers, TestAppContext};
use sheprd_client::ConnectTarget;

const NOTE: &str = "<sheprd device=\"desktop\" paths=\"here\" sum=\"0\">\nBe brief.\n</sheprd>";

fn prompt(id: u64, text: &str, note: Option<&str>) -> Entry {
    Entry {
        id: EntryId(id),
        kind: EntryKind::User {
            text: text.into(),
            files: 0,
            note: note.map(str::to_owned),
        },
    }
}

#[gpui::test]
fn as_sent_shows_each_prompts_note_and_hides_it_again(cx: &mut TestAppContext) {
    let (view, _peer, cx) = overlay::open(cx, ConnectTarget::Local);
    let program = Program::new();
    let _stream = overlay::attach_with(
        &view,
        &program,
        snapshot(
            cursor(1, 0),
            vec![prompt(1, "first", Some(NOTE)), prompt(2, "second", None)],
        ),
        cx,
    );
    wait_until(|| {
        cx.run_until_parked();
        overlay::poll(&view, &program, cx);
        cx.debug_bounds("chat-as-sent").is_some()
    });
    assert!(cx.debug_bounds("chat-sent-note-1").is_none());
    assert!(cx.debug_bounds("chat-as-sent-banner").is_none());

    let toggle = cx.debug_bounds("chat-as-sent").unwrap();
    cx.simulate_click(toggle.center(), Modifiers::default());
    wait_until(|| {
        cx.run_until_parked();
        overlay::poll(&view, &program, cx);
        cx.debug_bounds("chat-sent-note-1").is_some()
    });
    assert!(cx.debug_bounds("chat-as-sent-banner").is_some());
    // A prompt that went alone shows nothing more.
    assert!(cx.debug_bounds("chat-sent-note-2").is_none());
    let note = cx.debug_bounds("chat-sent-note-1").unwrap();
    let copy = cx.debug_bounds("chat-sent-copy-1").unwrap();
    assert!(note.contains(&copy.center()));
    cx.simulate_click(copy.center(), Modifiers::default());
    assert_eq!(
        cx.update(|_, cx| cx.read_from_clipboard().unwrap().text().unwrap()),
        NOTE
    );

    let toggle = cx.debug_bounds("chat-as-sent").unwrap();
    cx.simulate_click(toggle.center(), Modifiers::default());
    wait_until(|| {
        cx.run_until_parked();
        overlay::poll(&view, &program, cx);
        cx.debug_bounds("chat-sent-note-1").is_none()
    });
    assert!(cx.debug_bounds("chat-as-sent-banner").is_none());
}
