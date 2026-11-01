use super::*;
use control::{Change, ControlCommand, DocCommand, Text};

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
fn doc_commands_parse() {
    assert_eq!(
        doc(&["doc", "read", "todo.md", "--section", "goals"]).unwrap(),
        DocCommand::Read {
            path: "todo.md".into(),
            section: Some("goals".into())
        }
    );
    assert_eq!(
        doc(&[
            "doc",
            "todo",
            "add",
            "todo.md",
            "--version",
            "3",
            "buy",
            "milk"
        ])
        .unwrap(),
        DocCommand::TodoAdd {
            path: "todo.md".into(),
            change: Change {
                version: 3,
                section: None
            },
            text: Text::Inline("buy milk".into()),
        }
    );
    assert_eq!(
        doc(&[
            "doc",
            "append",
            "todo.md",
            "--version",
            "3",
            "--",
            "- [ ] item"
        ])
        .unwrap(),
        DocCommand::Append {
            path: "todo.md".into(),
            change: Change {
                version: 3,
                section: None
            },
            text: Text::Inline("- [ ] item".into()),
        }
    );
    assert_eq!(
        doc(&[
            "doc",
            "edit",
            "a.md",
            "--version",
            "2",
            "--old",
            "- [ ] x",
            "--new",
            "- [x] x"
        ])
        .unwrap(),
        DocCommand::Edit {
            path: "a.md".into(),
            change: Change {
                version: 2,
                section: None
            },
            old: "- [ ] x".into(),
            new: "- [x] x".into(),
        }
    );
    assert_eq!(
        doc(&[
            "doc",
            "todo",
            "uncheck",
            "a.md",
            "--version",
            "2",
            "--section",
            "s",
            "4"
        ])
        .unwrap(),
        DocCommand::TodoCheck {
            path: "a.md".into(),
            change: Change {
                version: 2,
                section: Some("s".into())
            },
            todo: "4".into(),
            done: false,
        }
    );
    assert_eq!(
        doc(&["doc", "diff", "a.md", "--since", "4"]).unwrap(),
        DocCommand::Diff {
            path: "a.md".into(),
            since: 4
        }
    );
}

#[test]
fn a_change_without_a_version_is_refused() {
    assert!(doc(&["doc", "todo", "add", "a.md", "x"]).is_err());
    assert!(doc(&["doc", "append", "a.md", "--version", "0", "x"]).is_err());
    assert!(doc(&["doc", "edit", "a.md", "--version", "1", "--old", "x"]).is_err());
    assert!(doc(&["doc", "frobnicate", "a.md"]).is_err());
}
