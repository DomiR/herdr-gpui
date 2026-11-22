use super::*;
use control::{BriefSource, ChatCommand, ControlCommand};

fn new(args: &[&str]) -> Result<ChatCommand, CliError> {
    LaunchOptions::parse(args.iter().copied()).map(|options| match options.mode {
        LaunchMode::Control(invocation) => match invocation.command {
            ControlCommand::Chat(command) => command,
            command => panic!("{args:?} parsed as {command:?}"),
        },
        mode => panic!("{args:?} parsed as {mode:?}"),
    })
}

#[test]
fn chat_new_takes_a_brief_as_text_a_file_or_standard_input() {
    assert_eq!(
        new(&["chat", "new", "codex", "--prompt", "Fix the relay"]).unwrap(),
        ChatCommand::New {
            agent: Some(sheprd_chat_protocol::Agent::Codex),
            pane: None,
            brief: Some(BriefSource::Text("Fix the relay".into())),
        }
    );
    assert_eq!(
        new(&[
            "chat",
            "new",
            "--prompt-file",
            "/tmp/brief.md",
            "--pane",
            "w1:p2"
        ])
        .unwrap(),
        ChatCommand::New {
            agent: None,
            pane: Some("w1:p2".into()),
            brief: Some(BriefSource::File("/tmp/brief.md".into())),
        }
    );
    assert_eq!(
        new(&["chat", "new", "--prompt-file", "-"]).unwrap(),
        ChatCommand::New {
            agent: None,
            pane: None,
            brief: Some(BriefSource::File("-".into())),
        }
    );
}

#[test]
fn chat_new_takes_one_brief_with_a_value() {
    assert!(matches!(
        new(&["chat", "new", "--prompt", "a", "--prompt-file", "b"]),
        Err(CliError::Conflicting("--prompt", "--prompt-file"))
    ));
    assert!(matches!(
        new(&["chat", "new", "--prompt"]),
        Err(CliError::MissingValue("--prompt"))
    ));
    assert!(matches!(
        new(&["chat", "new", "--prompt-file", "--pane", "w1:p2"]),
        Err(CliError::MissingValue("--prompt-file"))
    ));
    assert!(matches!(
        new(&["chat", "new", "--prompt", "a", "--prompt", "b"]),
        Err(CliError::DuplicateOption("--prompt"))
    ));
}

#[cfg(unix)]
#[test]
fn a_brief_file_path_need_not_be_utf8() {
    use std::os::unix::ffi::OsStrExt as _;
    let path = std::ffi::OsStr::from_bytes(b"/tmp/\xffbrief.md");
    let options = LaunchOptions::parse([
        std::ffi::OsStr::new("chat"),
        std::ffi::OsStr::new("new"),
        std::ffi::OsStr::new("--prompt-file"),
        path,
    ])
    .unwrap();
    let LaunchMode::Control(invocation) = options.mode else {
        panic!("not a control command");
    };
    assert_eq!(
        invocation.command,
        ControlCommand::Chat(ChatCommand::New {
            agent: None,
            pane: None,
            brief: Some(BriefSource::File(path.to_owned())),
        })
    );
}
