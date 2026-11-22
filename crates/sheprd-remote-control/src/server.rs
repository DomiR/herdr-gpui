//! `herdr-remote-control serve`: loads the state directory (certificate authority,
//! devices, VAPID key), connects to the local Herdr daemon, and serves the
//! web app over HTTPS until Sheprd asks it to stop.

mod addresses;
mod bonjour;
mod control;
pub(crate) use control::version;
mod listener;
mod live;
mod notifier;
pub(crate) use notifier::forget;
mod options;

pub use crate::push::Privacy;
pub use options::{Certificate, FleetAction, Invocation, Options, ParseError, Relay, USAGE, parse};

use crate::{
    Result,
    auth::{Auth, Limits},
    hub::Hub,
    push::{Sender, Vapid},
    tls::{self, Authority, Names, Resolver},
    web::{self, State},
};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    net::IpAddr,
    sync::{Arc, Mutex, RwLock},
    time::{Duration, SystemTime},
};
use tokio::sync::watch;

/// How often the served addresses are checked, so a new network or
/// Tailscale coming up gets a certificate that names it.
const ADDRESS_CHECK: Duration = Duration::from_secs(30);
/// How often idle devices and expired pictures are forgotten.
const IDLE_CHECK: Duration = Duration::from_secs(60 * 60);
/// Who push services may contact about this sender.
const PUSH_SUBJECT: &str = "https://github.com/sheprd-labs/sheprd";
/// How long shutdown waits for open connections, such as phones' live
/// streams, before the daemon exits anyway.
const SHUTDOWN_GRACE: Duration = Duration::from_secs(2);

/// The control endpoint's answer, for other modules' tests.
#[cfg(test)]
pub(crate) fn answer_for_tests(
    state: &State,
    stop: &watch::Sender<bool>,
    request: crate::control::Request,
) -> crate::control::Response {
    control::answer(state, stop, request)
}

/// Every device's APNs registration, for other modules' tests.
#[cfg(test)]
pub(crate) fn registrations_for_tests(
    state: &State,
) -> Vec<(crate::control::DeviceId, crate::apns::Registration)> {
    live::registrations(state)
}

/// Runs the daemon on the current Tokio runtime until it is told to stop.
/// A daemon already serving the state directory leaves this one nothing to
/// do: it returns at once, so the process exits cleanly and the service
/// manager, which relaunches only a failed daemon, leaves it down. Failing
/// instead would retry every few seconds for as long as the other one runs,
/// filling the log. Sheprd stops the other one and starts the service when
/// it restarts or updates the daemon.
pub async fn run(options: Options) -> Result<()> {
    std::fs::create_dir_all(&options.state_dir)?;
    if control::serving(&options.state_dir) {
        tracing::warn!(
            state_dir = %options.state_dir.display(),
            "Another remote control daemon serves this state directory; exiting"
        );
        return Ok(());
    }
    let bind = options.bind;
    // Relays may be shared with this computer while it runs, so loopback,
    // where relay sessions are served, is always certified.
    let relayed = true;
    let (authority, resolver, names) = match &options.certificate {
        Certificate::Own => {
            let names = Names::current().bound_to(bind);
            let authority = Authority::load_or_create(&options.state_dir)?;
            let resolver =
                Resolver::issue(&authority, certified(&names, relayed), SystemTime::now())?;
            (Some(authority), resolver, names)
        }
        Certificate::Files { cert, key } => {
            let resolver = Resolver::from_files(cert, key)?;
            // Phones reach a user's certificate by the names it lists.
            let names = Names::from_certificate(resolver.current().cert[0].as_ref()).bound_to(bind);
            (None, resolver, names)
        }
    };
    let fingerprint = match &authority {
        Some(authority) => authority.fingerprint().to_owned(),
        None => colon_hex(&Sha256::digest(resolver.current().cert[0].as_ref())),
    };
    let resolver = Arc::new(resolver);
    let config = Arc::new(tls::server_config(resolver.clone())?);
    let push = Vapid::load_or_create(&options.state_dir)
        .and_then(|vapid| Sender::new(vapid, PUSH_SUBJECT))
        .ok();
    let mut auth = Auth::load(&options.state_dir, Limits::default())?;
    // Desktops paired over Bluetooth log in over SSH through the relays.
    match crate::ssh_access::SshAccess::of_user() {
        Ok(access) => auth.set_ssh_access(access),
        Err(error) => tracing::info!(error = %error, "desktops cannot be let in over SSH"),
    }
    let apps = auth.apps();
    let apns = crate::apns::Delivery::new(options.push_relay)?;
    tracing::info!(delivery = apns.name(), "APNs pushes");
    // A key an earlier release used, or copied here when linking a host, is
    // left on disk for its owner and never read: only the push relay holds
    // the key APNs takes for the app.
    let unused_key = sheprd_apns::dir(&options.state_dir);
    if unused_key.join("AuthKey.p8").exists() {
        tracing::info!(
            directory = %unused_key.display(),
            "an APNs key here is no longer used: iPhone pushes go only through the push relay"
        );
    }
    // Only a daemon whose pushes go through the push relay revokes handles there.
    let push_relay_revocations = apns.push_relays().is_some().then(|| {
        let (revocations, receiver) = crate::apns::push_relay_revoke::channel();
        auth.set_push_relay_revocations(revocations);
        receiver
    });
    // Only origins this computer's own authority serves may be remembered.
    let seen = match &authority {
        Some(authority) => web::Origins::load(&options.state_dir, authority.fingerprint()),
        None => web::Origins::none(),
    };
    let hub = Hub::start(
        options.herdr_socket.clone(),
        options.chat_dir.clone(),
        options.wsl_catalog.clone(),
        options.core_endpoint.clone(),
        &options.state_dir,
    )?;
    let (pull_requests, _) = watch::channel(Arc::new(Default::default()));
    let load = crate::load::start();
    // Kept even with no relay set, for ones another computer shares later.
    // Only a computer told to listen on relays fails without its keys.
    let relay = match crate::relay::Relays::load(&options.state_dir, options.relays.clone()) {
        Ok(relay) => Some(Arc::new(relay)),
        Err(error) if options.relays.is_empty() => {
            tracing::warn!(%error, "Could not load the relay keys; relays shared later are ignored");
            None
        }
        Err(error) => return Err(error.into()),
    };
    // A second daemon on this state directory stops here, before it
    // changes anything the running one keeps.
    let keys = auth.ssh_access().ok().map(|access| &access.keys);
    let fleet =
        crate::fleet::Fleet::for_daemon(&options.state_dir, web::now_ms(), keys)?.map(Arc::new);
    let state = Arc::new(State {
        apns,
        apps,
        auth: Mutex::new(auth),
        authority,
        changes: Default::default(),
        computer: options::computer_name(),
        connected: Mutex::new(HashMap::new()),
        favorites: web::Favorites::load(crate::app::favorites::file(&options.state_dir)),
        fingerprint,
        fleet,
        github: crate::native::changes::github::Reader::new(options.github_cli),
        hub,
        lending: crate::native::github::Lending::new(
            options.github_hosts.clone(),
            sheprd_github::cli::token_for,
        ),
        live_activities: options.live_activities,
        load,
        look: web::Look::load(&options.state_dir),
        names: RwLock::new(names),
        near: Mutex::default(),
        opened: web::attention::Opened::default(),
        port: options.port,
        previews: Default::default(),
        privacy: options.privacy,
        pull_requests,
        push,
        pushes: Default::default(),
        relay,
        reports: Mutex::new(web::report_limit()),
        seen: Mutex::new(seen),
        shortcuts: web::Shortcuts::load(crate::app::shortcuts::file(&options.state_dir)),
        tags: web::Tags::load(crate::app::tags::file(&options.state_dir)),
        typed: web::Typed::default(),
        updates: Mutex::default(),
        uploads: web::Uploads::new(&options.state_dir),
    });
    remember_origins(&state);
    // The port first: a second daemon fails here before it touches the
    // first one's control socket.
    let tcp = listener::bind(options.bind, options.port)?;
    tracing::info!(
        version = env!("CARGO_PKG_VERSION"),
        port = options.port,
        "Remote control started"
    );
    let (stop, stopped) = watch::channel(false);
    control::serve(&options.state_dir, state.clone(), stop.clone())?;
    // Only once this daemon owns the port and the control socket: a second
    // copy started on the same state must not delete the first one's
    // pictures.
    state.uploads.remove_leftovers();
    tokio::spawn(notifier::run(state.clone()));
    if let Some(receiver) = push_relay_revocations {
        tokio::spawn(revoke_push_relay_handles(state.clone(), receiver));
    }
    tokio::spawn(crate::fleet::grants::follow(state.clone()));
    if let Some(fleet) = state.fleet.clone() {
        tokio::spawn(crate::fleet::removals::flood(fleet));
    }
    tokio::spawn(web::follow_favorite_boot(state.clone()));
    tokio::spawn(crate::native::github::checkouts::follow(state.clone()));
    tokio::spawn(crate::native::previews::follow(state.clone()));
    tokio::spawn(crate::native::github::follow_gh_hosts(state.clone()));
    if state.github.enabled() {
        tokio::spawn(crate::native::changes::github::badges(state.clone()));
    }
    // A user's certificate lists fixed names; only the daemon's own follows
    // the computer's addresses.
    if state.authority.is_some() {
        tokio::spawn(watch_addresses(
            state.clone(),
            resolver.clone(),
            bind,
            relayed,
        ));
    }
    tokio::spawn(expire(state.clone(), options.device_idle));
    // On macOS a second mDNS responder beside mDNSResponder is invisible
    // to browsers on the same Mac and contends for its host name; there
    // Sheprd's `herdr-nearby` helper registers the service through the
    // system while the pairing dialog is open.
    if options.bonjour && !cfg!(target_os = "macos") {
        tokio::spawn(bonjour::advertise(state.clone(), stopped.clone()));
    }
    let router = web::router(state.clone());
    if let Some(relay) = &state.relay {
        // The phone reaches a relay session at `https://127.0.0.1:PORT`
        // through its loopback proxy: the certificate for that address,
        // which `certified` has issued whatever the bind address.
        let mut tls = (*config).clone();
        tls.cert_resolver = resolver.at(IpAddr::V4(std::net::Ipv4Addr::LOCALHOST));
        let served = crate::relay::Served {
            router: router.clone(),
            state: state.clone(),
            tls: Arc::new(tls),
        };
        relay.start(served, stopped.clone());
    }
    let app = router.into_make_service_with_connect_info::<web::Peer>();
    let mut shutdown = stopped.clone();
    let serve = axum::serve(listener::TlsListener::new(tcp, config, resolver), app)
        .with_graceful_shutdown(async move {
            let _ = shutdown.wait_for(|stop| *stop).await;
        })
        .into_future();
    until_stopped(serve, stopped, SHUTDOWN_GRACE).await?;
    crate::control::release(&options.state_dir);
    Ok(())
}

/// Runs `serve` until it ends, or until `grace` after a stop. A phone's
/// live stream never ends on its own, so graceful shutdown waits only so
/// long for open connections before the daemon exits anyway.
async fn until_stopped(
    serve: impl Future<Output = std::io::Result<()>>,
    mut stopped: watch::Receiver<bool>,
    grace: Duration,
) -> std::io::Result<()> {
    tokio::select! {
        served = serve => served,
        () = async move {
            let _ = stopped.wait_for(|stop| *stop).await;
            tokio::time::sleep(grace).await;
        } => Ok(()),
    }
}

/// The names certificates are issued for: the ones advertised and, when
/// relays are configured, loopback, where every relay session is served.
/// A daemon bound to one address would otherwise have no certificate for
/// the address the phone's relay proxy is reached at.
fn certified(advertised: &Names, relayed: bool) -> Names {
    if relayed {
        advertised.clone().with_relay_loopback()
    } else {
        advertised.clone()
    }
}

/// Adds certificates for new addresses and renews existing ones independently.
async fn watch_addresses(state: Arc<State>, resolver: Arc<Resolver>, bind: IpAddr, relayed: bool) {
    let mut ticker = tokio::time::interval(ADDRESS_CHECK);
    loop {
        ticker.tick().await;
        let Ok(names) = tokio::task::spawn_blocking(move || Names::current().bound_to(bind)).await
        else {
            continue;
        };
        let (refresh_state, refresh_resolver, refresh_names) =
            (state.clone(), resolver.clone(), certified(&names, relayed));
        let refreshed = tokio::task::spawn_blocking(move || match &refresh_state.authority {
            Some(authority) => refresh_resolver
                .refresh(authority, &refresh_names, SystemTime::now())
                .map(|_| ()),
            None => Ok(()),
        })
        .await;
        if !matches!(refreshed, Ok(Ok(()))) {
            tracing::warn!(error = ?refreshed, "Could not refresh remote certificates; keeping the current addresses");
            continue;
        }
        if let Ok(mut current) = state.names.write() {
            *current = names;
        }
        let state = state.clone();
        let _ = tokio::task::spawn_blocking(move || remember_origins(&state)).await;
    }
}

/// Adds the origins served now to the ones an installed app may still
/// send. Blocking; a failure to save only forgets them at the next start.
fn remember_origins(state: &State) {
    let origins = state.origins();
    let _ = state
        .seen
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .remember(&origins);
}

async fn expire(state: Arc<State>, idle: Option<Duration>) {
    let idle_ms = idle.map(|idle| u64::try_from(idle.as_millis()).unwrap_or(u64::MAX));
    let mut ticker = tokio::time::interval(IDLE_CHECK);
    loop {
        ticker.tick().await;
        let state = state.clone();
        let _ = tokio::task::spawn_blocking(move || sweep(&state, web::now_ms(), idle_ms)).await;
    }
}

/// Revokes unpaired devices' push relay handles through `state`'s push relays
/// (`apns::push_relay_revoke`).
async fn revoke_push_relay_handles(
    state: Arc<State>,
    receiver: tokio::sync::mpsc::Receiver<Vec<crate::apns::Handle>>,
) {
    let revoke = move |handle: crate::apns::Handle| {
        let state = state.clone();
        async move {
            match state.apns.push_relays() {
                Some(relays) => relays.revoke(&handle, web::now_ms()).await,
                None => sheprd_apns::RelayOutcome::Unavailable,
            }
        }
    };
    crate::apns::push_relay_revoke::run(receiver, revoke, crate::apns::push_relay_revoke::log)
        .await;
}

/// One tick of [`expire`]: drops old pictures, and devices away for
/// `idle_ms`; with no limit, devices stay until revoked. Blocking.
fn sweep(state: &State, now_ms: u64, idle_ms: Option<u64>) {
    // Pictures expire on their own schedule, whether or not another
    // upload comes to sweep them.
    state.uploads.prune(now_ms);
    if let Some(idle_ms) = idle_ms {
        let _ = state.auth().expire_idle(now_ms, idle_ms);
    }
}

/// Uppercase colon-separated hex, as certificate fingerprints read.
fn colon_hex(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|byte| format!("{byte:02X}"))
        .collect::<Vec<_>>()
        .join(":")
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests;
