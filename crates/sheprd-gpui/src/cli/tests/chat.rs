use super::*;
use control::{ChatCommand, ControlCommand, Invocation, Switch};

fn parse(args: &[&str]) -> Result<Invocation, CliError> {
    LaunchOptions::parse(args.iter().copied()).map(|options| match options.mode {
        LaunchMode::Control(invocation) => invocation,
        mode => panic!("{args:?} parsed as {mode:?}"),
    })
}

fn command(args: &[&str]) -> ControlCommand {
    parse(args).unwrap().command
}

#[test]
fn chat_lists_by_default() {
    assert_eq!(
        command(&["chat"]),
        ControlCommand::Chat(ChatCommand::Status)
    );
    assert_eq!(
        command(&["chat", "status"]),
        ControlCommand::Chat(ChatCommand::Status)
    );
    let json = parse(&["chat", "--json", "--window", "7"]).unwrap();
    assert!(json.json);
    assert_eq!(json.window, Some(7));
}

#[test]
fn chat_shows_or_hides_a_pane() {
    for (verb, switch) in [
        ("show", Switch::On),
        ("hide", Switch::Off),
        ("toggle", Switch::Toggle),
    ] {
        assert_eq!(
            command(&["chat", verb]),
            ControlCommand::Chat(ChatCommand::View { switch, pane: None })
        );
    }
    assert_eq!(
        command(&["chat", "hide", "--pane", "w1:p2"]),
        ControlCommand::Chat(ChatCommand::View {
            switch: Switch::Off,
            pane: Some("w1:p2".into())
        })
    );
}

#[test]
fn chat_refuses_what_it_does_not_know() {
    assert!(parse(&["chat", "prompt", "hello"]).is_err());
    assert!(parse(&["chat", "show", "extra"]).is_err());
    assert!(matches!(
        command(&["chat", "--help"]),
        ControlCommand::Help(Some("chat"))
    ));
}

#[test]
fn chat_convert_takes_an_optional_pane() {
    assert_eq!(
        command(&["chat", "convert"]),
        ControlCommand::Chat(ChatCommand::Convert { pane: None })
    );
    assert_eq!(
        command(&["chat", "convert", "--pane", "w1:p2"]),
        ControlCommand::Chat(ChatCommand::Convert {
            pane: Some("w1:p2".into())
        })
    );
    assert!(parse(&["chat", "convert", "claude"]).is_err());
}

#[test]
fn chat_new_names_a_built_in_agent() {
    assert_eq!(
        command(&["chat", "new", "claude"]),
        ControlCommand::Chat(ChatCommand::New {
            agent: Some(sheprd_chat_protocol::Agent::Claude),
            pane: None,
            brief: None,
        })
    );
    assert_eq!(
        command(&["chat", "new", "--pane", "w1:p2", "codex"]),
        ControlCommand::Chat(ChatCommand::New {
            agent: Some(sheprd_chat_protocol::Agent::Codex),
            pane: Some("w1:p2".into()),
            brief: None,
        })
    );
    assert!(matches!(
        parse(&["chat", "new", "cursor"]),
        Err(CliError::InvalidValue("AGENT"))
    ));
    // No agent: the caller's chat's own, with its settings.
    assert_eq!(
        command(&["chat", "new", "--pane", "w1:p2"]),
        ControlCommand::Chat(ChatCommand::New {
            agent: None,
            pane: Some("w1:p2".into()),
            brief: None,
        })
    );
    assert!(parse(&["chat", "new", "claude", "codex"]).is_err());
}

#[test]
fn chat_favorite_toggles_unless_told() {
    assert_eq!(
        command(&["chat", "favorite"]),
        ControlCommand::Chat(ChatCommand::Favorite {
            switch: Switch::Toggle,
            pane: None
        })
    );
    for (word, switch) in [
        ("on", Switch::On),
        ("off", Switch::Off),
        ("toggle", Switch::Toggle),
    ] {
        assert_eq!(
            command(&["chat", "favorite", word, "--pane", "w1:p2"]),
            ControlCommand::Chat(ChatCommand::Favorite {
                switch,
                pane: Some("w1:p2".into())
            })
        );
    }
    assert_eq!(
        command(&["chat", "favorite", "--pane", "core:t7", "off"]),
        ControlCommand::Chat(ChatCommand::Favorite {
            switch: Switch::Off,
            pane: Some("core:t7".into())
        })
    );
    assert!(parse(&["chat", "favorite", "maybe"]).is_err());
    assert!(parse(&["chat", "favorite", "on", "extra"]).is_err());
}
