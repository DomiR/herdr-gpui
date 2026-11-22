//! Starting the relay: its secrets, its store, and its socket.

use crate::{
    Backend, Error, Options, Result,
    attest::Verifier,
    credential::Issuer,
    handle::Keyring,
    limit::Rates,
    relay::Relay,
    store::{Memory, Sqlite, Store},
    web::{self, Source},
};
#[cfg(feature = "postgres")]
use secrecy::SecretString;
use secrecy::{ExposeSecret, SecretSlice};
use sheprd_apns::{Config, Host, Sender, is_private, load_secret_key};
use sheprd_dashboard::auth::Account;
use std::{
    fs,
    net::SocketAddr,
    path::{Path, PathBuf},
    time::Duration,
};
use tokio::net::TcpListener;

/// How often expired revocations are dropped.
const PURGE: Duration = Duration::from_secs(60 * 60);
const MIN_SECRET: usize = 32;

/// Runs the relay until it is told to stop, or with `--check`, loads its
/// secrets and stops without opening a store or a socket.
pub async fn serve(options: Options) -> Result<()> {
    if options.check {
        return Secrets::load(&options.secrets).map(drop);
    }
    match &options.backend {
        Backend::Memory => run(&options, Memory::default()).await,
        Backend::Postgres => postgres(&options).await,
        Backend::Sqlite(path) => run(&options, Sqlite::open(path)?).await,
    }
}

#[cfg(feature = "postgres")]
async fn postgres(options: &Options) -> Result<()> {
    use crate::store::Postgres;
    let path = options.secrets.join("postgres-url");
    let bytes = read_private(&path)?;
    let url = std::str::from_utf8(bytes.expose_secret())
        .map(|url| SecretString::from(url.trim()))
        .map_err(|_| Error::SecretSize { path: path.clone() })?;
    let ca = options.secrets.join("postgres-ca.pem");
    let ca = ca.exists().then_some(ca.as_path());
    run(options, Postgres::connect(&url, ca).await?).await
}

#[cfg(not(feature = "postgres"))]
async fn postgres(_: &Options) -> Result<()> {
    Err(Error::NoPostgres)
}

/// Everything the relay reads from its secrets directory, each checked as
/// the relay uses it.
pub(crate) struct Secrets {
    attestations: Verifier,
    /// The dashboard's and `/metrics`'s token, when the operator set one.
    dashboard: Option<Account>,
    environment: Host,
    issuer: Issuer,
    keys: Keyring,
    sender: Sender,
}

impl Secrets {
    pub(crate) fn load(dir: &Path) -> Result<Self> {
        let config = Config::load(dir)?.ok_or_else(|| Error::NoApnsConfig {
            path: dir.to_owned(),
        })?;
        let environment = config.host;
        // The app App Attest names: the APNs topic is its bundle id.
        let attestations = Verifier::apple(&format!("{}.{}", config.team_id, config.topic));
        let previous = dir.join("relay-key-previous.pem");
        let keys = Keyring::new(
            load_secret_key(&dir.join("relay-key.pem"))?,
            previous
                .exists()
                .then(|| load_secret_key(&previous))
                .transpose()?,
        );
        let secret = dir.join("credential-secret");
        let issuer = Issuer::new(read_secret(&secret)?.expose_secret())
            .ok_or(Error::SecretSize { path: secret })?;
        Ok(Self {
            attestations,
            dashboard: read_dashboard_token(&dir.join("dashboard-token"))?,
            environment,
            issuer,
            keys,
            sender: Sender::new(config)?,
        })
    }
}

async fn run(options: &Options, store: impl Store) -> Result<()> {
    let Secrets {
        attestations,
        dashboard,
        environment,
        issuer,
        keys,
        sender,
    } = Secrets::load(&options.secrets)?;
    let relay = Relay::new(
        sender,
        environment,
        issuer,
        keys,
        store.clone(),
        Rates::default(),
        attestations,
    );
    let source = if options.behind_proxy {
        Source::Proxy
    } else {
        Source::Peer
    };
    tokio::spawn(purge(store));
    let listener = TcpListener::bind(options.listen)
        .await
        .map_err(|source| Error::Bind {
            address: options.listen,
            source,
        })?;
    if dashboard.is_some() {
        tracing::info!("the dashboard is at /dashboard, and /metrics takes its token");
    } else {
        tracing::info!("the dashboard and /metrics are off: add dashboard-token to the secrets");
    }
    tracing::info!(address = %options.listen, host = environment.name(), "the relay is listening");
    axum::serve(
        listener,
        web::router(relay, source, dashboard).into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(stopped())
    .await
    .map_err(Error::Serve)
}

/// A secret file's bytes, refusing one others can read.
fn read_private(path: &Path) -> Result<SecretSlice<u8>> {
    let read = |source| Error::SecretRead {
        path: path.to_owned(),
        source,
    };
    if !is_private(&fs::symlink_metadata(path).map_err(read)?) {
        return Err(Error::NotPrivate {
            path: path.to_owned(),
        });
    }
    fs::read(path).map(SecretSlice::from).map_err(read)
}

/// The credential secret, refusing a file others can read or a short one.
pub(crate) fn read_secret(path: &Path) -> Result<SecretSlice<u8>> {
    let owned = || -> PathBuf { path.to_owned() };
    let text = read_private(path)?;
    let hex = text.expose_secret().trim_ascii();
    let bytes: Option<Vec<u8>> = hex
        .chunks(2)
        .map(|pair| {
            let pair = std::str::from_utf8(pair).ok()?;
            (pair.len() == 2).then(|| u8::from_str_radix(pair, 16).ok())?
        })
        .collect();
    bytes
        .filter(|bytes| bytes.len() >= MIN_SECRET)
        .map(SecretSlice::from)
        .ok_or_else(|| Error::SecretSize { path: owned() })
}

/// The dashboard's account, from the token at `path`, or `None` when there
/// is no file: the dashboard is optional. A file others can read, or a
/// short token, is refused.
pub(crate) fn read_dashboard_token(path: &Path) -> Result<Option<Account>> {
    if !path.exists() {
        return Ok(None);
    }
    let bytes = read_private(path)?;
    let token = std::str::from_utf8(bytes.expose_secret()).map_err(|_| Error::NotText {
        path: path.to_owned(),
    })?;
    Ok(Some(Account::new(token.trim())?))
}

async fn purge(store: impl Store) {
    let mut ticks = tokio::time::interval(PURGE);
    loop {
        ticks.tick().await;
        if let Err(error) = store.purge(web::now_ms() / 1000).await {
            tracing::warn!(%error, "could not drop expired revocations");
        }
    }
}

/// Resolves on Ctrl-C or, where there is one, `SIGTERM`, which is what
/// `docker stop` sends.
async fn stopped() {
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
