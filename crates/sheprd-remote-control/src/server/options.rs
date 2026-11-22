//! Command-line options, parsed from OS strings before any runtime starts.

use crate::{app::config::ActivityDetail, push::Privacy, relay_label::RelayLabel};
use std::{
    ffi::OsString,
    net::{IpAddr, Ipv6Addr},
    path::PathBuf,
    time::Duration,
};

pub const DEFAULT_PORT: u16 = 7443;
const DEFAULT_IDLE_DAYS: u64 = 30;
const MAX_IDLE_DAYS: u64 = 3650;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Certificate {
    /// The daemon's own certificate authority.
    Own,
    /// A certificate chain and key the user provides, as PEM files.
    Files { cert: PathBuf, key: PathBuf },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Options {
    /// Where to listen: every interface (`[::]`, dual stack) or one address.
    pub bind: IpAddr,
    /// Whether to advertise over Bonjour, for phones nearby to find it.
    pub bonjour: bool,
    pub certificate: Certificate,
    /// The folder chat sockets live in, as Sheprd resolved it; `None`
    /// resolves it from this process's environment.
    pub chat_dir: Option<PathBuf>,
    /// The desktop endpoint, when explicitly passed by its service plan.
    pub core_endpoint: crate::core_target::CoreTarget,
    /// How long a device may stay away before it is forgotten; `None`
    /// keeps it until revoked.
    pub device_idle: Option<Duration>,
    /// Explicit permission to read this machine's GitHub CLI credential.
    pub github_cli: bool,
    /// GitHub Enterprise Server hosts besides those `gh` is signed in to,
    /// whose remotes a phone is told are on GitHub.
    pub github_hosts: Vec<String>,
    /// The Herdr daemon's client socket, as Sheprd resolved it; `None`
    /// discovers it from this process's environment. A service manager's
    /// environment may lack the shell's `XDG_CONFIG_HOME`.
    pub herdr_socket: Option<PathBuf>,
    pub live_activities: ActivityDetail,
    pub port: u16,
    pub privacy: Privacy,
    /// Whether APNs pushes go through the Sheprd push relay, the only way
    /// they leave: on unless `--no-push-relay`.
    pub push_relay: bool,
    /// The relays to listen on, so paired phones reach this computer from
    /// any network; none when empty.
    pub relays: Vec<Relay>,
    pub state_dir: PathBuf,
    /// The saved WSL distributions, as Sheprd resolved them; `None` resolves
    /// them from this process's environment.
    pub wsl_catalog: Option<PathBuf>,
}

/// A relay to listen on, and what the user calls it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Relay {
    pub endpoint: sheprd_relay::Endpoint,
    pub label: RelayLabel,
}

impl From<sheprd_relay::Endpoint> for Relay {
    fn from(endpoint: sheprd_relay::Endpoint) -> Self {
        Self {
            endpoint,
            label: RelayLabel::default(),
        }
    }
}

#[derive(Debug, thiserror::Error, Eq, PartialEq)]
pub enum ParseError {
    // keep-sorted start
    #[error("--cert and --key go together")]
    CertificatePair,
    #[error("--core-socket and --core-session go together")]
    CorePair,
    #[error("{0} needs a value")]
    Missing(&'static str),
    #[error("{option} names no relay: put it after a --relay")]
    NoRelay { option: &'static str },
    #[error("--state-dir is required")]
    NoStateDir,
    #[error("--relay {relay} has a label it cannot use")]
    RelayLabel {
        relay: String,
        #[source]
        source: crate::relay_label::Error,
    },
    #[error("unknown option {0}")]
    Unknown(String),
    #[error("{option} has an invalid value")]
    Value { option: &'static str },
    // keep-sorted end
}

/// What the command line asks for.
#[derive(Debug, Eq, PartialEq)]
pub enum Invocation {
    Serve(Box<Options>),
    /// Adds a phone another computer's Sheprd allowed, read as JSON from
    /// standard input (`app::enroll`).
    Enroll {
        state_dir: Option<PathBuf>,
    },
    /// Keeps a phone's relay keys another computer's Sheprd carried here,
    /// read as JSON from standard input (`app::relay_enroll`).
    RelayEnroll {
        state_dir: Option<PathBuf>,
    },
    /// Prints the running daemon's push status as JSON, or nothing when
    /// none answers (`app::push_status`).
    PushStatus {
        state_dir: Option<PathBuf>,
    },
    /// Revokes a device that other computer enrolled.
    Revoke {
        state_dir: Option<PathBuf>,
        device: String,
    },
    /// Lists the relays this computer listens on, taking any another
    /// computer shares on standard input (`control::relays::Offer`).
    Relays {
        state_dir: Option<PathBuf>,
        offer: bool,
    },
    /// Reads this computer's favorite agents, making the change
    /// (`control::favorites::Change`) given on standard input, if any.
    Favorites {
        state_dir: Option<PathBuf>,
        offer: bool,
    },
    /// Reads this computer's text shortcuts, offering any given on standard
    /// input.
    Shortcuts {
        state_dir: Option<PathBuf>,
        offer: bool,
    },
    /// Reads this computer's tags, offering any given on standard input.
    Tags {
        state_dir: Option<PathBuf>,
        offer: bool,
    },
    /// Adds a nearby Sheprd as `name`, with Sheprd carrying Bluetooth on
    /// stdio (`crate::desktop`).
    DesktopPair {
        state_dir: Option<PathBuf>,
        name: String,
        /// From a pairing link on stdin, not over Bluetooth.
        link: bool,
    },
    /// Carries `ssh` to a machine added over Bluetooth, `r-ID.sheprd.invalid`,
    /// through its relays: its `ProxyCommand`.
    RelayConnect {
        state_dir: Option<PathBuf>,
        host: String,
    },
    /// The fleet's halves Sheprd runs over SSH (`crate::fleet`).
    Fleet {
        state_dir: Option<PathBuf>,
        action: FleetAction,
    },
    Help,
    Version,
}

/// What `fleet` does.
#[derive(Debug, Eq, PartialEq)]
pub enum FleetAction {
    /// Takes `control::fleet::sync::Apply` on standard input and prints
    /// what came of it, with what the caller lacks.
    Apply,
    /// Prints this computer's bundle signed for the fleet with this id, for
    /// the member about to admit it.
    Bundle { fleet: String },
    /// Prints this computer as a Sheprd host, its record signed by its
    /// certificate authority, for the member about to share it.
    Intro,
    /// Forgets the fleet here and the lines and records it wrote.
    Scrub,
}

pub const USAGE: &str = "\
Usage: sheprd-remote-control serve --state-dir DIR [options]
       sheprd-remote-control enroll [--state-dir DIR] < REQUEST.json
       sheprd-remote-control revoke --device ID [--state-dir DIR]
       sheprd-remote-control relay-enroll [--state-dir DIR] < REQUEST.json
       sheprd-remote-control push-status [--state-dir DIR]
       sheprd-remote-control tags [--offer] [--state-dir DIR] [< TAGS.json]
       sheprd-remote-control shortcuts [--offer] [--state-dir DIR] [< SHORTCUTS.json]
       sheprd-remote-control favorites [--offer] [--state-dir DIR] [< CHANGE.json]
       sheprd-remote-control relays [--offer] [--state-dir DIR] [< RELAYS.json]
       sheprd-remote-control fleet apply [--state-dir DIR] < APPLY.json
       sheprd-remote-control fleet bundle --fleet ID [--state-dir DIR]
       sheprd-remote-control fleet intro [--state-dir DIR]
       sheprd-remote-control fleet scrub [--state-dir DIR]
       sheprd-remote-control desktop-pair --name NAME [--link] [--state-dir DIR]
       sheprd-remote-control relay-connect [--state-dir DIR] HOST

Serves this computer's Herdr chats to paired phones over HTTPS. Sheprd
starts it when Remote control is on.

Options:
  --state-dir DIR          Where keys, devices, and certificates live (required)
  --port N                 HTTPS port (default 7443)
  --bind all|ADDRESS       Listen on every interface (default) or one address
  --cert FILE --key FILE   Use this certificate chain and key instead of the
                           daemon's own certificate authority
  --herdr-socket PATH      The Herdr daemon's client socket (default: discovered);
                           other running Herdr sessions are followed too
  --core-socket PATH --core-session NAME
                           The desktop core endpoint (default: discovered)
  --chat-dir DIR           Where chat sockets live (default: discovered)
  --github-cli             Read PRs/checks/logs using this host's gh sign-in;
                           permits paired phones to read accessible repositories
  --github-host HOST       Count HOST as a GitHub Enterprise Server host for
                           phones, besides those gh is signed in to; repeatable
  --wsl-catalog FILE       The saved WSL distributions (default: discovered)
  --privacy generic|excerpt
                           Whether notifications may quote the chat (default excerpt)
  --live-activities detailed|generic
                           Whether Live Activities name the chat and its step
                           (default detailed)
  --device-idle-days N     Forget devices unseen this long, 0 for never
                           (default 30)
  --no-push-relay          Never send iPhone pushes through the Sheprd push
                           relay (push.sheprd.org), the only way they leave
                           this computer. By default alerts and Live
                           Activities go sealed through it, to phones that
                           opted in. --push-relay is accepted as the default
  --relay URL              Listen on this relay too, so paired phones reach
                           this computer from any network; repeatable
  --relay-name NAME        What the --relay before it is called, such as fra3
  --relay-description TEXT A line about the --relay before it, such as
                           not for public use
  --relay-tag TAG          A tag for the --relay before it; repeatable
  --no-bonjour             Do not advertise over Bonjour; phones then pair
                           by QR code only (on macOS, Sheprd advertises
                           while its pairing dialog is open instead)
  -h, --help               Show this help
  -V, --version            Show the version

enroll and revoke are what Sheprd on another computer runs here over SSH,
so a phone paired there also reaches this computer. enroll starts the
daemon when it does not run and prints this computer's answer as JSON.
relay-enroll keeps a phone's relay keys, signed with its own key here, so
the phone reaches this computer through its relays, and prints the outcome
as JSON; it needs the daemon running.
push-status prints how the daemon running here sends iPhone pushes, as
JSON (its mode, and how the last alert and Live Activity push went), so
Sheprd on another computer can show it; it prints nothing when no daemon
answers.
relays --offer takes {\"source\", \"relays\"}, the relays Sheprd on
another computer listens on, replacing what that computer shared before:
the daemon listens on them too, beside its own [remote] relays.
desktop-pair adds a nearby Sheprd that is on another network: Sheprd
runs it and carries its Bluetooth frames as JSON lines on stdio. The
other computer is then reached through its relays, and relay-connect is
the ProxyCommand ssh runs for it. With --link, it adds a Sheprd on any
network from a pairing link that computer's Pair device copied, read as
the first JSON line on stdin, never from the command line.
fleet apply and fleet bundle are the fleet's halves Sheprd on another
computer runs here over SSH: apply takes signed fleet operations and
prints what this computer holds that the caller lacks; bundle prints this
computer's keys, signed, for the member about to admit it; intro prints
this computer as a host, signed by its certificate authority, for a
member about to share it; scrub forgets the fleet here, with the
authorized_keys lines and relay records it wrote, without signing
anything: for a computer the fleet removed, or a fleet that is gone.
Their state directory defaults to the one Sheprd on this computer uses.
";

pub fn parse(args: impl IntoIterator<Item = OsString>) -> Result<Invocation, ParseError> {
    let mut args = args.into_iter().skip(1);
    match args.next().as_deref().and_then(|arg| arg.to_str()) {
        Some("serve") => {}
        Some(verb @ ("enroll" | "revoke")) => return parse_host(verb == "enroll", args),
        // Takes `enroll`'s options, `--state-dir` alone.
        Some("relay-enroll") => {
            return parse_host(true, args).map(|invocation| match invocation {
                Invocation::Enroll { state_dir } => Invocation::RelayEnroll { state_dir },
                other => other,
            });
        }
        // `--state-dir` alone too.
        Some("push-status") => {
            return parse_host(true, args).map(|invocation| match invocation {
                Invocation::Enroll { state_dir } => Invocation::PushStatus { state_dir },
                other => other,
            });
        }
        Some("favorites") => {
            return parse_edited(args, |state_dir, offer| Invocation::Favorites {
                state_dir,
                offer,
            });
        }
        Some("relays") => {
            return parse_edited(args, |state_dir, offer| Invocation::Relays {
                state_dir,
                offer,
            });
        }
        Some("shortcuts") => {
            return parse_edited(args, |state_dir, offer| Invocation::Shortcuts {
                state_dir,
                offer,
            });
        }
        Some("tags") => {
            return parse_edited(args, |state_dir, offer| Invocation::Tags {
                state_dir,
                offer,
            });
        }
        Some("fleet") => return parse_fleet(args),
        Some(verb @ ("desktop-pair" | "relay-connect")) => {
            return parse_desktop(verb == "desktop-pair", args);
        }
        Some("-h" | "--help") | None => return Ok(Invocation::Help),
        Some("-V" | "--version") => return Ok(Invocation::Version),
        Some(other) => return Err(ParseError::Unknown(other.to_owned())),
    }
    let mut state_dir = None;
    let mut port = DEFAULT_PORT;
    let mut bind = IpAddr::V6(Ipv6Addr::UNSPECIFIED);
    let (mut cert, mut key) = (None, None);
    let mut privacy = Privacy::default();
    let mut live_activities = ActivityDetail::default();
    let mut idle_days = DEFAULT_IDLE_DAYS;
    let (mut herdr_socket, mut chat_dir, mut wsl_catalog) = (None, None, None);
    let (mut core_socket, mut core_session) = (None, None);
    let mut bonjour = true;
    let mut github_cli = false;
    let mut github_hosts = Vec::new();
    let mut push_relay = true;
    let mut relays = Vec::new();
    while let Some(arg) = args.next() {
        let name = arg.to_str().unwrap_or_default().to_owned();
        let mut value = |option: &'static str| args.next().ok_or(ParseError::Missing(option));
        let text = |value: OsString, option: &'static str| {
            value
                .into_string()
                .map_err(|_| ParseError::Value { option })
        };
        match name.as_str() {
            "--state-dir" => state_dir = Some(PathBuf::from(value("--state-dir")?)),
            "--port" => {
                port = text(value("--port")?, "--port")?
                    .parse()
                    .ok()
                    .filter(|port| *port != 0)
                    .ok_or(ParseError::Value { option: "--port" })?;
            }
            "--bind" => {
                let address = text(value("--bind")?, "--bind")?;
                if address != "all" {
                    bind = address
                        .parse()
                        .map_err(|_| ParseError::Value { option: "--bind" })?;
                }
            }
            "--cert" => cert = Some(PathBuf::from(value("--cert")?)),
            "--key" => key = Some(PathBuf::from(value("--key")?)),
            "--herdr-socket" => herdr_socket = Some(PathBuf::from(value("--herdr-socket")?)),
            "--core-socket" => core_socket = Some(PathBuf::from(value("--core-socket")?)),
            "--core-session" => {
                core_session = Some(text(value("--core-session")?, "--core-session")?)
            }
            "--chat-dir" => chat_dir = Some(PathBuf::from(value("--chat-dir")?)),
            "--wsl-catalog" => wsl_catalog = Some(PathBuf::from(value("--wsl-catalog")?)),
            "--privacy" => {
                privacy = match text(value("--privacy")?, "--privacy")?.as_str() {
                    "generic" => Privacy::Generic,
                    "excerpt" => Privacy::Excerpt,
                    _ => {
                        return Err(ParseError::Value {
                            option: "--privacy",
                        });
                    }
                };
            }
            "--live-activities" => {
                live_activities =
                    match text(value("--live-activities")?, "--live-activities")?.as_str() {
                        "detailed" => ActivityDetail::Detailed,
                        "generic" => ActivityDetail::Generic,
                        _ => {
                            return Err(ParseError::Value {
                                option: "--live-activities",
                            });
                        }
                    };
            }
            "--device-idle-days" => {
                idle_days = text(value("--device-idle-days")?, "--device-idle-days")?
                    .parse()
                    .ok()
                    .filter(|days| *days <= MAX_IDLE_DAYS)
                    .ok_or(ParseError::Value {
                        option: "--device-idle-days",
                    })?;
            }
            "--relay" => {
                let endpoint = text(value("--relay")?, "--relay")?;
                relays.push(
                    sheprd_relay::Endpoint::parse(&endpoint)
                        .map_err(|_| ParseError::Value { option: "--relay" })?
                        .into(),
                );
            }
            "--relay-name" => {
                let name = text(value("--relay-name")?, "--relay-name")?;
                last_label(&mut relays, "--relay-name")?.name = Some(name);
            }
            "--relay-description" => {
                let description = text(value("--relay-description")?, "--relay-description")?;
                last_label(&mut relays, "--relay-description")?.description = Some(description);
            }
            "--relay-tag" => {
                let tag = text(value("--relay-tag")?, "--relay-tag")?;
                last_label(&mut relays, "--relay-tag")?.tags.push(tag);
            }
            "--no-bonjour" => bonjour = false,
            "--github-cli" => github_cli = true,
            // Service definitions written while it was off by default.
            "--push-relay" => push_relay = true,
            "--no-push-relay" => push_relay = false,
            "--github-host" => {
                let host = text(value("--github-host")?, "--github-host")?;
                if !crate::control::github::valid_host(&host) {
                    return Err(ParseError::Value {
                        option: "--github-host",
                    });
                }
                github_hosts.push(host);
            }
            "-h" | "--help" => return Ok(Invocation::Help),
            _ => return Err(ParseError::Unknown(name)),
        }
    }
    for relay in &relays {
        relay
            .label
            .check()
            .map_err(|source| ParseError::RelayLabel {
                relay: relay.endpoint.to_string(),
                source,
            })?;
    }
    let certificate = match (cert, key) {
        (None, None) => Certificate::Own,
        (Some(cert), Some(key)) => Certificate::Files { cert, key },
        _ => return Err(ParseError::CertificatePair),
    };
    let core_endpoint = match (core_socket, core_session) {
        (None, None) => crate::core_target::CoreTarget::Discover,
        (Some(socket), Some(session)) => {
            crate::core_target::CoreTarget::Endpoint(sheprd_core_client::Endpoint {
                socket,
                session,
            })
        }
        _ => return Err(ParseError::CorePair),
    };
    Ok(Invocation::Serve(Box::new(Options {
        bind,
        bonjour,
        certificate,
        chat_dir,
        core_endpoint,
        device_idle: (idle_days != 0).then(|| Duration::from_secs(idle_days * 24 * 60 * 60)),
        github_cli,
        github_hosts,
        herdr_socket,
        live_activities,
        port,
        privacy,
        push_relay,
        relays,
        state_dir: state_dir.ok_or(ParseError::NoStateDir)?,
        wsl_catalog,
    })))
}

/// The label of the `--relay` that `option` follows.
fn last_label<'a>(
    relays: &'a mut [Relay],
    option: &'static str,
) -> Result<&'a mut RelayLabel, ParseError> {
    relays
        .last_mut()
        .map(|relay| &mut relay.label)
        .ok_or(ParseError::NoRelay { option })
}

/// `enroll [--state-dir DIR]` and `revoke --device ID [--state-dir DIR]`.
fn parse_host(
    enroll: bool,
    mut args: impl Iterator<Item = OsString>,
) -> Result<Invocation, ParseError> {
    let (mut state_dir, mut device) = (None, None);
    while let Some(arg) = args.next() {
        let name = arg.to_str().unwrap_or_default().to_owned();
        match name.as_str() {
            "--state-dir" => {
                state_dir = Some(PathBuf::from(
                    args.next().ok_or(ParseError::Missing("--state-dir"))?,
                ));
            }
            "--device" if !enroll => {
                device = Some(
                    args.next()
                        .ok_or(ParseError::Missing("--device"))?
                        .into_string()
                        .ok()
                        .filter(|device| !device.is_empty())
                        .ok_or(ParseError::Value { option: "--device" })?,
                );
            }
            "-h" | "--help" => return Ok(Invocation::Help),
            _ => return Err(ParseError::Unknown(name)),
        }
    }
    if enroll {
        return Ok(Invocation::Enroll { state_dir });
    }
    Ok(Invocation::Revoke {
        state_dir,
        device: device.ok_or(ParseError::Missing("--device"))?,
    })
}

/// `desktop-pair --name NAME [--link] [--state-dir DIR]` and
/// `relay-connect [--state-dir DIR] HOST`.
fn parse_desktop(
    pair: bool,
    mut args: impl Iterator<Item = OsString>,
) -> Result<Invocation, ParseError> {
    let (mut state_dir, mut name, mut host, mut link) = (None, None, None, false);
    while let Some(arg) = args.next() {
        let text = arg.to_str().unwrap_or_default().to_owned();
        match text.as_str() {
            "--state-dir" => {
                state_dir = Some(PathBuf::from(
                    args.next().ok_or(ParseError::Missing("--state-dir"))?,
                ));
            }
            "--name" if pair => {
                name = Some(
                    args.next()
                        .ok_or(ParseError::Missing("--name"))?
                        .into_string()
                        .map_err(|_| ParseError::Value { option: "--name" })?,
                );
            }
            "--link" if pair => link = true,
            "-h" | "--help" => return Ok(Invocation::Help),
            _ if !pair && host.is_none() && !text.starts_with('-') && !text.is_empty() => {
                host = Some(text);
            }
            _ => return Err(ParseError::Unknown(text)),
        }
    }
    if pair {
        return Ok(Invocation::DesktopPair {
            state_dir,
            name: name.ok_or(ParseError::Missing("--name"))?,
            link,
        });
    }
    Ok(Invocation::RelayConnect {
        state_dir,
        host: host.ok_or(ParseError::Missing("HOST"))?,
    })
}

/// `fleet apply|bundle [--fleet ID] [--state-dir DIR]`.
fn parse_fleet(mut args: impl Iterator<Item = OsString>) -> Result<Invocation, ParseError> {
    let verb = args
        .next()
        .and_then(|verb| verb.to_str().map(str::to_owned))
        .ok_or(ParseError::Missing("apply, bundle, intro, or scrub"))?;
    let (mut state_dir, mut fleet) = (None, None);
    while let Some(arg) = args.next() {
        let name = arg.to_str().unwrap_or_default().to_owned();
        match name.as_str() {
            "--state-dir" => {
                state_dir = Some(PathBuf::from(
                    args.next().ok_or(ParseError::Missing("--state-dir"))?,
                ));
            }
            "--fleet" => {
                let id = args.next().ok_or(ParseError::Missing("--fleet"))?;
                fleet = Some(
                    id.to_str()
                        .filter(|id| {
                            id.len() == 64 && id.bytes().all(|byte| byte.is_ascii_hexdigit())
                        })
                        .ok_or(ParseError::Value { option: "--fleet" })?
                        .to_owned(),
                );
            }
            "-h" | "--help" => return Ok(Invocation::Help),
            _ => return Err(ParseError::Unknown(name)),
        }
    }
    let action = match verb.as_str() {
        "apply" if fleet.is_none() => FleetAction::Apply,
        "intro" if fleet.is_none() => FleetAction::Intro,
        "scrub" if fleet.is_none() => FleetAction::Scrub,
        "bundle" => FleetAction::Bundle {
            fleet: fleet.ok_or(ParseError::Missing("--fleet"))?,
        },
        "-h" | "--help" => return Ok(Invocation::Help),
        _ => return Err(ParseError::Unknown(verb)),
    };
    Ok(Invocation::Fleet { state_dir, action })
}

/// `tags`, `shortcuts`, `favorites`, or `relays`, then `[--offer] [--state-dir DIR]`, as `made`.
fn parse_edited(
    mut args: impl Iterator<Item = OsString>,
    made: fn(Option<PathBuf>, bool) -> Invocation,
) -> Result<Invocation, ParseError> {
    let (mut state_dir, mut offer) = (None, false);
    while let Some(arg) = args.next() {
        let name = arg.to_str().unwrap_or_default().to_owned();
        match name.as_str() {
            "--state-dir" => {
                state_dir = Some(PathBuf::from(
                    args.next().ok_or(ParseError::Missing("--state-dir"))?,
                ));
            }
            "--offer" => offer = true,
            "-h" | "--help" => return Ok(Invocation::Help),
            _ => return Err(ParseError::Unknown(name)),
        }
    }
    Ok(made(state_dir, offer))
}

/// The computer's name as its owner knows it.
pub(crate) fn computer_name() -> String {
    #[cfg(unix)]
    {
        let name = rustix::system::uname()
            .nodename()
            .to_string_lossy()
            .into_owned();
        let name = name.strip_suffix(".local").unwrap_or(&name).to_owned();
        if !name.is_empty() {
            return name;
        }
    }
    std::env::var("COMPUTERNAME").unwrap_or_else(|_| "This computer".to_owned())
}
