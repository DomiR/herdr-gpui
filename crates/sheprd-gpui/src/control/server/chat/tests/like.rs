use super::*;

#[gpui::test]
fn a_new_chat_without_an_agent_needs_a_caller_in_a_chat(cx: &mut gpui::TestAppContext) {
    let (_view, cx) = cx.add_window_view(fixture_window);
    cx.cx.update(|cx| {
        let outside = serve(
            Call {
                caller: Caller {
                    pane_id: Some("w1:p1".into()),
                    ..Caller::default()
                },
                op: ChatOp::New {
                    agent: None,
                    pane: None,
                    prompt: None,
                },
            },
            cx,
        );
        let Response::Error { code, message } = outside else {
            panic!("refused: {outside:?}");
        };
        assert_eq!(code, ErrorCode::InvalidRequest);
        assert!(message.contains("Name an agent"), "{message}");
    });
}
