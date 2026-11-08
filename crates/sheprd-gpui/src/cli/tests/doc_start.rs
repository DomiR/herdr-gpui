use super::*;

use control::{Change, ControlCommand, DocCommand};

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
fn start_parses_like_a_todo_state_change() {
    assert_eq!(
        doc(&[
            "doc",
            "todo",
            "start",
            "TODO.md",
            "--version",
            "5",
            "--section",
            "next",
            "2",
        ])
        .unwrap(),
        DocCommand::TodoStart {
            path: "TODO.md".into(),
            change: Change {
                version: 5,
                section: Some("next".into()),
            },
            todo: "2".into(),
        }
    );
}
