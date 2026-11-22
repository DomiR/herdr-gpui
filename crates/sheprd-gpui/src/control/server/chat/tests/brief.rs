use super::*;

#[gpui::test]
fn a_brief_no_chat_takes_is_deleted(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    view.update(cx, |view, _| {
        view.chats.program = crate::chat::Program::Missing;
    });
    let brief = Brief::write("Fix the relay", None).unwrap();
    let file = brief.file.clone();
    let response = cx.cx.update(|cx| {
        start_new(
            &Caller::default(),
            Some("claude".into()),
            None,
            Some(brief),
            cx,
        )
    });
    assert!(matches!(response, Response::Error { .. }), "{response:?}");
    cx.run_until_parked();
    assert!(!file.exists());
    // Nothing is left for the next start to carry.
    view.update(cx, |view, _| assert!(view.chats.brief.is_none()));
}

#[gpui::test]
fn a_brief_with_an_unknown_agent_is_deleted(cx: &mut gpui::TestAppContext) {
    let (_view, cx) = cx.add_window_view(fixture_window);
    let brief = Brief::write("Fix the relay", None).unwrap();
    let file = brief.file.clone();
    let response = cx.cx.update(|cx| {
        start_new(
            &Caller::default(),
            Some("cursor".into()),
            None,
            Some(brief),
            cx,
        )
    });
    assert_eq!(code(&response), Some(ErrorCode::InvalidRequest));
    cx.run_until_parked();
    assert!(!file.exists());
}

#[gpui::test]
fn a_brief_is_only_answered_once_written(cx: &mut gpui::TestAppContext) {
    let (_view, cx) = cx.add_window_view(fixture_window);
    cx.cx.update(|cx| {
        let response = serve(
            call(ChatOp::New {
                agent: Some("claude".into()),
                pane: None,
                prompt: Some("Fix the relay".into()),
            }),
            cx,
        );
        assert_eq!(code(&response), Some(ErrorCode::Failed));
    });
}

#[test]
fn a_refused_brief_start_exits_as_unsupported() {
    assert_eq!(
        start_code(&crate::chat::StartError::BriefRemote),
        ErrorCode::Unsupported
    );
}
