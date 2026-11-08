use super::*;

use control::{ControlCommand, DocCommand};

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
fn list_parses_repeatable_tags_and_normalizes_them() {
    assert_eq!(
        doc(&[
            "doc", "todo", "list", "TODO.md", "--tag", "@High", "--tag", "due"
        ])
        .unwrap(),
        DocCommand::TodoList {
            path: "TODO.md".into(),
            section: None,
            tags: vec!["high".into(), "due".into()],
        }
    );
}

#[test]
fn list_rejects_invalid_or_too_many_tags() {
    assert!(matches!(
        doc(&["doc", "todo", "list", "TODO.md", "--tag", "high.tag"]),
        Err(CliError::InvalidValue("--tag"))
    ));
    let mut args = vec!["doc", "todo", "list", "TODO.md"];
    for _ in 0..=sheprd_control_protocol::protocol::doc::MAX_TAGS {
        args.extend(["--tag", "tag"]);
    }
    assert!(matches!(doc(&args), Err(CliError::InvalidValue("--tag"))));
}
