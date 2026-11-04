use super::*;

use control::{Change, ControlCommand, DocCommand};
use sheprd_control_protocol::protocol::doc::TaskState;

fn doc(args: &[&str]) -> Result<DocCommand, CliError> {
    let options = LaunchOptions::parse(args.iter().copied())?;
    match options.mode {
        LaunchMode::Control(invocation) => match invocation.command {
            ControlCommand::Doc(command) => Ok(command),
            other => panic!("expected a doc command, got {other:?}"),
        },
        mode => panic!("{args:?} parsed as {mode:?}"),
    }
}

#[test]
fn cancel_and_uncancel_parse_like_check_and_uncheck() {
    assert_eq!(
        doc(&["doc", "todo", "cancel", "a.md", "--version", "2", "4"]).unwrap(),
        DocCommand::TodoState {
            path: "a.md".into(),
            change: Change {
                version: 2,
                section: None,
            },
            todo: "4".into(),
            state: TaskState::Cancelled,
        }
    );
    assert_eq!(
        doc(&[
            "doc",
            "todo",
            "uncancel",
            "a.md",
            "--version",
            "2",
            "--section",
            "next",
            "4",
        ])
        .unwrap(),
        DocCommand::TodoState {
            path: "a.md".into(),
            change: Change {
                version: 2,
                section: Some("next".into()),
            },
            todo: "4".into(),
            state: TaskState::Open,
        }
    );
    assert_eq!(
        doc(&[
            "doc",
            "todo",
            "cancel",
            "a.md",
            "--version",
            "2",
            "--undo",
            "4"
        ])
        .unwrap(),
        DocCommand::TodoState {
            path: "a.md".into(),
            change: Change {
                version: 2,
                section: None,
            },
            todo: "4".into(),
            state: TaskState::Open,
        }
    );
    assert!(matches!(
        doc(&[
            "doc",
            "todo",
            "uncancel",
            "a.md",
            "--version",
            "2",
            "--undo",
            "4",
        ]),
        Err(CliError::Unexpected { .. })
    ));
}
