use super::*;
use crate::bench::options::DEFAULT_NAME;

fn parse(args: &[&str]) -> std::result::Result<Mode, Error> {
    let args: Vec<String> = args.iter().map(|arg| (*arg).to_owned()).collect();
    Mode::parse(&args)
}

#[test]
fn no_mode_and_help_print_the_usage() {
    assert_eq!(parse(&[]).unwrap(), Mode::Help);
    assert_eq!(parse(&["--help"]).unwrap(), Mode::Help);
    assert_eq!(parse(&["run", "-h"]).unwrap(), Mode::Help);
}

#[test]
fn serve_takes_a_name() {
    assert_eq!(
        parse(&["serve"]).unwrap(),
        Mode::Serve {
            name: DEFAULT_NAME.to_owned()
        }
    );
    assert_eq!(
        parse(&["serve", "--name", "desk"]).unwrap(),
        Mode::Serve {
            name: "desk".to_owned()
        }
    );
    // What only `run` reads is refused rather than ignored.
    assert!(matches!(
        parse(&["serve", "--minutes", "1"]),
        Err(Error::UnknownOption(option)) if option == "--minutes"
    ));
}

#[test]
fn run_reads_every_option_and_defaults_the_rest() {
    assert_eq!(parse(&["run"]).unwrap(), Mode::Run(Plan::default()));
    assert_eq!(
        parse(&[
            "run",
            "--name",
            "desk",
            "--label",
            "5 m, one wall",
            "--echoes",
            "20",
            "--bytes",
            "1000",
            "--minutes",
            "0",
        ])
        .unwrap(),
        Mode::Run(Plan {
            name: "desk".to_owned(),
            label: "5 m, one wall".to_owned(),
            echoes: 20,
            bytes: 1000,
            minutes: 0,
        })
    );
}

#[test]
fn the_download_is_capped_at_what_the_serving_side_sends() {
    let Mode::Run(plan) = parse(&["run", "--bytes", "4000000000"]).unwrap() else {
        panic!("not a run");
    };
    assert_eq!(plan.bytes, crate::bench::wire::MAX_PULL);
}

#[test]
fn bad_arguments_name_what_is_wrong() {
    assert!(matches!(
        parse(&["listen"]),
        Err(Error::UnknownMode(mode)) if mode == "listen"
    ));
    assert!(matches!(
        parse(&["run", "--echoes"]),
        Err(Error::MissingValue("--echoes"))
    ));
    assert!(matches!(
        parse(&["run", "--minutes", "ten"]),
        Err(Error::InvalidNumber { option: "--minutes", value }) if value == "ten"
    ));
    assert!(matches!(
        parse(&["run", "--fast"]),
        Err(Error::UnknownOption(option)) if option == "--fast"
    ));
}
