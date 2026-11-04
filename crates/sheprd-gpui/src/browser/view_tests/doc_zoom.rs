use super::*;
use crate::{browser::Store, controls::Command, editor::EditorTarget};

fn run(view: &Entity<SheprdWindow>, command: Command, cx: &mut VisualTestContext) {
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.command(command, window, cx));
    });
    draw(cx);
}

#[gpui::test]
fn the_font_size_keys_zoom_the_focused_document_without_sizing_the_terminal(
    cx: &mut gpui::TestAppContext,
) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("document.md");
    std::fs::write(&path, "# Document\n\nText\n").unwrap();
    let target = EditorTarget { path, line: None };
    let (view, cx) = window(cx);
    let id = cx
        .update(|window, cx| view.update(cx, |view, cx| view.open_doc_view(&target, window, cx)))
        .unwrap();
    cx.run_until_parked();
    draw(cx);

    let editor = cx
        .update(|_, cx| view.read(cx).doc_views.get(&id).cloned())
        .unwrap();
    let terminal_size = view.read_with(cx, |view, _| view.config.terminal.size);
    let ui_size = view.read_with(cx, |view, _| view.config.ui.size);
    let zoom = |cx: &mut VisualTestContext| {
        cx.update(|_, cx| cx.global::<Store>().get(id).unwrap().zoom.percent())
    };
    let look_ui_size =
        |cx: &mut VisualTestContext| cx.update(|_, cx| editor.read(cx).look().ui.size);

    run(&view, Command::IncreaseFontSize, cx);
    run(&view, Command::IncreaseFontSize, cx);
    assert_eq!(zoom(cx), 125);
    assert_eq!(
        view.read_with(cx, |view, _| view.config.terminal.size),
        terminal_size
    );
    assert_eq!(look_ui_size(cx), ui_size * 1.25);

    run(&view, Command::DecreaseFontSize, cx);
    assert_eq!(zoom(cx), 110);
    run(&view, Command::ResetFontSize, cx);
    assert_eq!(zoom(cx), 100);
    assert_eq!(look_ui_size(cx), ui_size);
}
