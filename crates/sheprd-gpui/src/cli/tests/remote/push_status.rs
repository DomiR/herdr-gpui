use super::*;

#[test]
fn remote_push_status_parses_with_json_and_window() {
    let invocation = parse(&["remote", "push-status", "--json", "--window", "2"]).unwrap();
    assert_eq!(
        invocation.command,
        ControlCommand::Remote(RemoteCommand::PushStatus)
    );
    assert!(invocation.json);
    assert_eq!(invocation.window, Some(2));
    // It only reads: nothing after it is taken.
    assert!(parse(&["remote", "push-status", "on"]).is_err());
}
