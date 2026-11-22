use super::*;

fn mode(args: &[&str]) -> Result<LaunchMode, CliError> {
    LaunchOptions::parse(args.iter().copied()).map(|options| options.mode)
}

fn open(focus: bool) -> LaunchMode {
    browser(BrowserCommand::Open {
        target: "localhost:3000".into(),
        workspace: None,
        focus,
        placement: None,
    })
}

#[test]
fn browser_open_switches_to_the_tab_only_with_focus() {
    assert_eq!(
        mode(&["browser", "open", "localhost:3000"]).unwrap(),
        open(false)
    );
    assert_eq!(
        mode(&["browser", "open", "--focus", "localhost:3000"]).unwrap(),
        open(true)
    );
    assert_eq!(
        mode(&["browser", "open", "localhost:3000", "--focus"]).unwrap(),
        open(true)
    );
    // Scripts written for the old default keep working.
    assert_eq!(
        mode(&["browser", "open", "--no-focus", "localhost:3000"]).unwrap(),
        open(false)
    );
}

#[test]
fn page_reads_show_the_tab_only_with_focus() {
    let text = |focus| browser(BrowserCommand::Text { id: 3, focus });
    let shot = |focus| {
        browser(BrowserCommand::Screenshot {
            id: 3,
            path: "shot.png".into(),
            focus,
        })
    };
    assert_eq!(mode(&["browser", "text", "3"]).unwrap(), text(false));
    assert_eq!(
        mode(&["browser", "text", "3", "--focus"]).unwrap(),
        text(true)
    );
    assert_eq!(
        mode(&["browser", "text", "--focus", "3"]).unwrap(),
        text(true)
    );
    assert_eq!(
        mode(&["browser", "text", "3", "--no-focus"]).unwrap(),
        text(false)
    );
    assert_eq!(
        mode(&["browser", "screenshot", "3", "shot.png"]).unwrap(),
        shot(false)
    );
    assert_eq!(
        mode(&["browser", "screenshot", "3", "shot.png", "--focus"]).unwrap(),
        shot(true)
    );
    assert_eq!(
        mode(&["browser", "screenshot", "--focus", "3", "shot.png"]).unwrap(),
        shot(true)
    );
    assert_eq!(
        mode(&["browser", "text", "3", "--force"]).unwrap_err(),
        CliError::UnexpectedArgument("--force".into())
    );
    // Commands that never move focus take no such flag.
    for verb in ["navigate", "back", "close"] {
        let args: &[&str] = if verb == "navigate" {
            &["browser", verb, "3", "https://a.test", "--focus"]
        } else {
            &["browser", verb, "3", "--focus"]
        };
        assert_eq!(
            mode(args).unwrap_err(),
            CliError::UnexpectedArgument("--focus".into()),
            "{verb}"
        );
    }
}
