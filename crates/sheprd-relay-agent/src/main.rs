//! `sheprd-relay-agent`; see the crate documentation and `--help`.

use sheprd_relay_agent::{
    Error, Result, access,
    agent::{Agent, Options, Relay},
    authorized::Authorized,
    authorized_keys,
    cli::{self, Command, RelayArg},
    files, fleet,
    status::Report,
};
use std::{
    io::Read as _,
    net::{Ipv4Addr, SocketAddr},
    path::{Path, PathBuf},
    process::ExitCode,
    time::SystemTime,
};

/// The most of a listen token file read; tokens are short.
const MAX_TOKEN: u64 = 4 * 1024;

fn main() -> ExitCode {
    let invocation = match cli::parse(std::env::args_os().skip(1)) {
        Ok(invocation) => invocation,
        Err(error) => {
            eprintln!("sheprd-relay-agent: {error}");
            return ExitCode::from(2);
        }
    };
    match invocation.command {
        Command::Help => {
            print!("{}", cli::USAGE);
            return ExitCode::SUCCESS;
        }
        Command::Version => {
            println!("sheprd-relay-agent {}", env!("CARGO_PKG_VERSION"));
            return ExitCode::SUCCESS;
        }
        _ => {}
    }
    let result = invocation
        .state_dir
        .map_or_else(files::default_state_dir, Ok)
        .and_then(|state_dir| execute(invocation.command, &state_dir));
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!(
                "sheprd-relay-agent: {}",
                sheprd_relay_agent::error::chain(&error)
            );
            ExitCode::FAILURE
        }
    }
}

fn execute(command: Command, state_dir: &Path) -> Result<()> {
    match command {
        Command::Help | Command::Version => Ok(()),
        Command::Status { json } => {
            let report = Report::read(state_dir, SystemTime::now())?;
            if json {
                print_json(state_dir, &report)
            } else {
                print!("{}", report.text());
                Ok(())
            }
        }
        Command::FleetApply { authorized_keys } => {
            let mut bytes = Vec::new();
            std::io::stdin()
                .take(access::MAX_BYTES as u64 + 1)
                .read_to_end(&mut bytes)
                .map_err(|source| Error::Io {
                    path: PathBuf::from("-"),
                    source,
                })?;
            let keys = keys_path(authorized_keys)?;
            fleet::Held::apply_bytes(state_dir, Some(&keys), &bytes)?;
            print_json(state_dir, &Report::read(state_dir, SystemTime::now())?)
        }
        Command::FleetScrub { authorized_keys } => {
            let keys = keys_path(authorized_keys)?;
            fleet::Held::scrub(state_dir, Some(&keys))?;
            print_json(state_dir, &Report::read(state_dir, SystemTime::now())?)
        }
        Command::Authorize(entry) => {
            let path = state_dir.join(sheprd_relay_agent::authorized::FILE);
            let (dial, tunnel) = (entry.dial, entry.tunnel);
            Authorized::update(state_dir, |authorized| {
                authorized
                    .insert(entry)
                    .map_err(|_| Error::AuthorizedFull { path })?;
                // The fleet's members dial too: a desktop that would crowd
                // one out is refused.
                let held = fleet::Held::load(state_dir)?;
                fleet::fits(authorized, held.access.as_ref())?;
                Ok(true)
            })?;
            // A person enrolled it again: a removed member's having named
            // its keys no longer cuts it off.
            fleet::Held::release(state_dir, dial, tunnel)
        }
        Command::Revoke(dial) => {
            Authorized::update(state_dir, |authorized| Ok(authorized.remove(&dial)))
        }
        Command::Run(run) => {
            let relays = run
                .relays
                .into_iter()
                .map(relay)
                .collect::<Result<Vec<_>>>()?;
            let options = Options {
                // An account with nowhere to put the fleet's lines still
                // cuts a suspended member off in the tunnel.
                authorized_keys: keys_path(run.authorized_keys).ok(),
                forward: SocketAddr::from((Ipv4Addr::LOCALHOST, run.forward)),
                relays,
                state_dir: state_dir.to_owned(),
            };
            serve(options)
        }
    }
}

fn print_json(state_dir: &Path, report: &Report) -> Result<()> {
    let text = serde_json::to_string_pretty(report).map_err(|source| Error::StatusFormat {
        path: state_dir.to_owned(),
        source,
    })?;
    println!("{text}");
    Ok(())
}

/// Where the fleet's `authorized_keys` lines go: the path given, or the
/// user's.
fn keys_path(given: Option<PathBuf>) -> Result<PathBuf> {
    given.map_or_else(authorized_keys::of_user, Ok)
}

/// A relay with its token read from its file, trimmed.
fn relay(arg: RelayArg) -> Result<Relay> {
    let token = match &arg.token_file {
        Some(path) => {
            let bytes = files::read_bounded(path, MAX_TOKEN)?.ok_or_else(|| Error::Io {
                path: path.clone(),
                source: std::io::ErrorKind::NotFound.into(),
            })?;
            String::from_utf8_lossy(&bytes).trim().as_bytes().to_vec()
        }
        None => Vec::new(),
    };
    Ok(Relay {
        endpoint: arg.endpoint,
        token,
    })
}

/// Runs the agent until interrupted or terminated.
fn serve(options: Options) -> Result<()> {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_target(false)
        .init();
    let agent = Agent::load(options)?;
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(Error::Runtime)?;
    runtime.block_on(async move {
        let (stop, stopped) = tokio::sync::watch::channel(false);
        let running = tokio::spawn(agent.run(stopped));
        stop_signal().await;
        let _ = stop.send(true);
        let _ = running.await;
    });
    Ok(())
}

/// Resolves on Ctrl-C, or on SIGTERM, which a service manager sends.
async fn stop_signal() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{SignalKind, signal};
        if let Ok(mut terminate) = signal(SignalKind::terminate()) {
            tokio::select! {
                _ = tokio::signal::ctrl_c() => {}
                _ = terminate.recv() => {}
            }
            return;
        }
    }
    let _ = tokio::signal::ctrl_c().await;
}
