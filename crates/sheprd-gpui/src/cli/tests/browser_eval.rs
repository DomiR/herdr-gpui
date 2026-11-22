use super::*;
use control::ScriptSource;

fn mode(args: &[&str]) -> Result<LaunchMode, CliError> {
    LaunchOptions::parse(args.iter().copied()).map(|o| o.mode)
}

#[test]
fn browser_eval_takes_a_script_or_standard_input() {
    assert_eq!(
        mode(&["browser", "eval", "3", "document.title"]).unwrap(),
        browser(BrowserCommand::Eval {
            id: 3,
            script: ScriptSource::Inline("document.title".into()),
            wait: 15,
        })
    );
    assert_eq!(
        mode(&["browser", "eval", "3", "--wait", "120", "-"]).unwrap(),
        browser(BrowserCommand::Eval {
            id: 3,
            script: ScriptSource::Stdin,
            wait: 120,
        })
    );
    // The wait may follow the script.
    assert_eq!(
        mode(&["browser", "eval", "3", "-1", "--wait", "2"]).unwrap(),
        browser(BrowserCommand::Eval {
            id: 3,
            script: ScriptSource::Inline("-1".into()),
            wait: 2,
        })
    );
}

#[test]
fn browser_eval_refuses_what_it_cannot_run() {
    for (args, expected) in [
        (
            &["browser", "eval", "3"][..],
            CliError::MissingArgument {
                command: "browser eval",
                what: "SCRIPT (or - to read it from standard input)",
            },
        ),
        (
            &["browser", "eval", "x", "1"],
            CliError::InvalidValue("TAB (an ID from `browser list`)"),
        ),
        (
            &["browser", "eval", "3", "--wait", "0", "1"],
            CliError::InvalidWait,
        ),
        (
            &["browser", "eval", "3", "--wait", "601", "1"],
            CliError::InvalidWait,
        ),
        (
            &["browser", "eval", "3", "1", "2"],
            CliError::UnexpectedArgument("2".into()),
        ),
    ] {
        assert_eq!(mode(args).unwrap_err(), expected, "{args:?}");
    }
}

#[test]
fn browser_clear_data_names_one_tab() {
    assert_eq!(
        mode(&["browser", "clear-data", "5"]).unwrap(),
        browser(BrowserCommand::ClearData { id: 5 })
    );
    assert_eq!(
        mode(&["browser", "clear-data", "5", "6"]).unwrap_err(),
        CliError::UnexpectedArgument("6".into())
    );
}

#[test]
fn browser_logs_reads_and_may_clear() {
    assert_eq!(
        mode(&["browser", "logs", "4"]).unwrap(),
        browser(BrowserCommand::Logs {
            id: 4,
            clear: false
        })
    );
    assert_eq!(
        mode(&["browser", "logs", "4", "--clear"]).unwrap(),
        browser(BrowserCommand::Logs { id: 4, clear: true })
    );
    assert_eq!(
        mode(&["browser", "logs", "4", "--all"]).unwrap_err(),
        CliError::UnexpectedArgument("--all".into())
    );
}
