//! `sheprd-nearby bench`'s arguments.

use super::Error;

/// The name the serving side advertises, and the running side looks for.
pub(crate) const DEFAULT_NAME: &str = "sheprd-bench";

pub(crate) const USAGE: &str = "\
Usage: sheprd-nearby bench serve [--name NAME]
       sheprd-nearby bench run [--name NAME] [--label TEXT] [--echoes N]
                               [--bytes N] [--minutes N]

Measures Bluetooth LE between two computers over the nearby service's `pair`
characteristic, as nearby pairing uses it: writes with response one way,
notifications the other. Run `serve` on one computer and `run` on the other;
`run` prints one JSON report on standard output and its progress on standard
error. `serve` advertises like an open Pair device dialog, so a Sheprd
nearby lists it while it runs.

  --name NAME     the name `serve` advertises and `run` looks for
                  (default sheprd-bench)
  --label TEXT    where the run happened, copied into the report
  --echoes N      echo round trips to time (default 100)
  --bytes N       bytes to move each way (default 262144)
  --minutes N     minutes of one echo a second, for stability (default 10;
                  0 skips it)
";

#[derive(Debug, Eq, PartialEq)]
pub(crate) enum Mode {
    Help,
    Serve { name: String },
    Run(Plan),
}

/// What `run` measures.
#[derive(Debug, Eq, PartialEq)]
pub(crate) struct Plan {
    pub(crate) name: String,
    pub(crate) label: String,
    pub(crate) echoes: u32,
    pub(crate) bytes: u32,
    pub(crate) minutes: u32,
}

impl Default for Plan {
    fn default() -> Self {
        Self {
            name: DEFAULT_NAME.to_owned(),
            label: String::new(),
            echoes: 100,
            bytes: 256 * 1024,
            minutes: 10,
        }
    }
}

impl Mode {
    pub(crate) fn parse(args: &[String]) -> Result<Self, Error> {
        let Some((mode, rest)) = args.split_first() else {
            return Ok(Self::Help);
        };
        if rest.iter().any(|arg| arg == "-h" || arg == "--help") {
            return Ok(Self::Help);
        }
        let serve = match mode.as_str() {
            "-h" | "--help" | "help" => return Ok(Self::Help),
            "serve" => true,
            "run" => false,
            other => return Err(Error::UnknownMode(other.to_owned())),
        };
        let mut plan = Plan::default();
        let mut rest = rest.iter();
        while let Some(option) = rest.next() {
            let mut value = |name: &'static str| {
                rest.next()
                    .map(String::as_str)
                    .ok_or(Error::MissingValue(name))
            };
            match option.as_str() {
                "--name" => plan.name = value("--name")?.to_owned(),
                "--label" if !serve => plan.label = value("--label")?.to_owned(),
                "--echoes" if !serve => plan.echoes = number("--echoes", value("--echoes")?)?,
                "--bytes" if !serve => {
                    plan.bytes = number("--bytes", value("--bytes")?)?.min(super::wire::MAX_PULL);
                }
                "--minutes" if !serve => plan.minutes = number("--minutes", value("--minutes")?)?,
                other => return Err(Error::UnknownOption(other.to_owned())),
            }
        }
        Ok(if serve {
            Self::Serve { name: plan.name }
        } else {
            Self::Run(plan)
        })
    }
}

fn number(option: &'static str, value: &str) -> Result<u32, Error> {
    value.parse().map_err(|_| Error::InvalidNumber {
        option,
        value: value.to_owned(),
    })
}
