//! The daemon's fleet: members founding, admitting, and removing one
//! another on a real relay on loopback, each with a stand-in SSH server
//! and its own `authorized_keys`. Unix only, as `desktop`'s flow: a
//! member's bundle names its user from `USER`.

use super::*;
use crate::{
    control::fleet::sync::{Answer, Ask},
    relay::{
        Relays,
        tests::{listening, relay, served},
    },
    ssh_access::{AuthorizedKeys, SshAccess},
    web::{Shared, now_ms, tests::state},
};
use sheprd_relay::Endpoint;
use std::{net::SocketAddr, sync::Arc, time::Duration};
use tokio::{
    io::{AsyncReadExt as _, AsyncWriteExt as _},
    net::TcpListener,
    sync::watch,
};

#[cfg(unix)]
mod agent;
#[cfg(unix)]
mod agent_opening;
#[cfg(unix)]
mod aging;
#[cfg(unix)]
mod also_owed;
#[cfg(unix)]
mod atomic;
#[cfg(unix)]
mod bluetooth;
#[cfg(unix)]
mod catch_up;
#[cfg(unix)]
mod earlier_shapes;
#[cfg(unix)]
mod flood;
#[cfg(unix)]
mod folding;
#[cfg(unix)]
mod handover;
#[cfg(unix)]
mod held_agent;
#[cfg(unix)]
mod joining;
#[cfg(unix)]
mod leaving;
#[cfg(unix)]
mod link_intents;
#[cfg(unix)]
mod link_recovery;
#[cfg(unix)]
mod offline;
#[cfg(unix)]
mod opening;
#[cfg(unix)]
mod other_fleet;
#[cfg(unix)]
mod partial_join;
#[cfg(unix)]
mod phone_relays;
#[cfg(unix)]
mod phones;
#[cfg(unix)]
mod proof;
#[cfg(unix)]
mod records;
#[cfg(unix)]
mod rejected;
#[cfg(unix)]
mod removed_peer;
#[cfg(unix)]
mod restore;
#[cfg(unix)]
mod retry;
#[cfg(unix)]
mod route_claims;
#[cfg(unix)]
mod scrub;
#[cfg(unix)]
mod scrub_links;
#[cfg(unix)]
mod scrub_races;
#[cfg(unix)]
mod sessions;
#[cfg(unix)]
mod settling;
#[cfg(unix)]
mod shared_records;
#[cfg(unix)]
mod startup;
#[cfg(unix)]
mod store;
#[cfg(unix)]
mod unsaved;

/// A stand-in SSH server that greets each connection, then echoes.
async fn ssh_server() -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(async move {
        while let Ok((mut socket, _)) = listener.accept().await {
            tokio::spawn(async move {
                socket.write_all(b"SSH-2.0-test\r\n").await.unwrap();
                let mut buffer = [0; 1024];
                while let Ok(read) = socket.read(&mut buffer).await {
                    if read == 0 || socket.write_all(&buffer[..read]).await.is_err() {
                        break;
                    }
                }
            });
        }
    });
    address
}

/// One computer: its daemon's state with relays and a fleet, a stand-in
/// SSH server, and its own `authorized_keys`.
struct Node {
    dir: tempfile::TempDir,
    state: Shared,
    fleet: Arc<Fleet>,
    relays: Arc<Relays>,
    stop: Option<watch::Sender<bool>>,
}

impl Node {
    /// A computer that listens on the relay at `http`, not started yet.
    async fn new(http: SocketAddr, seed: u8) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let mut state = state(dir.path());
        let endpoint = Endpoint::parse(&format!("http://{http}")).unwrap();
        let relays = Arc::new(Relays::load(dir.path(), vec![endpoint.into()]).unwrap());
        let fleet = Arc::new(Fleet::load(dir.path(), now_ms()).unwrap());
        {
            let state = Arc::get_mut(&mut state).unwrap();
            state.relay = Some(Arc::clone(&relays));
            state.fleet = Some(Arc::clone(&fleet));
        }
        let host_key_dir = dir.path().join("etc-ssh");
        std::fs::create_dir(&host_key_dir).unwrap();
        std::fs::write(
            host_key_dir.join("ssh_host_ed25519_key.pub"),
            format!(
                "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAI{} root@node\n",
                char::from(b'A' + seed % 26).to_string().repeat(43)
            ),
        )
        .unwrap();
        let server = ssh_server().await;
        state.auth().set_ssh_access(SshAccess {
            host_key_dir,
            keys: AuthorizedKeys::at(dir.path().join("authorized_keys")),
            server,
        });
        Self {
            dir,
            state,
            fleet,
            relays,
            stop: None,
        }
    }

    /// Starts listening on the relay, applying the fleet's grants, and
    /// passing removals, as `server::run` does.
    async fn start(&mut self) {
        let (stop, stopped) = watch::channel(false);
        self.relays
            .start(served(self.dir.path(), &self.state).0, stopped);
        listening(&self.relays).await;
        tokio::spawn(grants::follow(self.state.clone()));
        tokio::spawn(removals::flood(self.fleet.clone()));
        self.stop = Some(stop);
    }

    fn ask(&self, ask: Ask) -> Answer {
        ask::answer(&self.state, &self.fleet, ask, now_ms()).unwrap()
    }

    fn view(&self) -> crate::control::fleet::View {
        ask::view(&self.fleet).unwrap()
    }

    fn me(&self) -> String {
        self.fleet.me().to_string()
    }

    /// Every operation this member holds.
    fn ops(&self) -> Vec<Op> {
        self.fleet
            .inner()
            .log
            .as_ref()
            .unwrap()
            .ops()
            .cloned()
            .collect()
    }

    /// The fleet lines in its `authorized_keys`.
    fn lines(&self) -> Vec<String> {
        std::fs::read_to_string(self.dir.path().join("authorized_keys"))
            .unwrap_or_default()
            .lines()
            .filter(|line| line.contains("sheprd-fleet:"))
            .map(str::to_owned)
            .collect()
    }
}

/// `joiner` joins `by`'s fleet over SSH: it signs its bundle for the
/// fleet, `by` admits it, and `by`'s log is handed to it.
fn join(by: &Node, joiner: &Node) {
    let fleet = by.view().fleet;
    let Answer::Bundle { bundle, signature } = joiner.ask(Ask::Bundle { fleet }) else {
        panic!("no bundle");
    };
    assert_eq!(
        by.ask(Ask::Admit {
            bundle,
            signature,
            method: crate::control::fleet::Method::Ssh,
        }),
        Answer::Done
    );
    joiner.fleet.receive(by.ops(), now_ms()).unwrap();
}

/// Waits until `check` holds, for up to `seconds`.
async fn until(seconds: u64, mut check: impl FnMut() -> bool) {
    for _ in 0..seconds * 20 {
        if check() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("never happened");
}

/// a, b, and c in one fleet, none started: no daemon task rewrites
/// grants or publishes a record behind a test that applies them itself.
/// Started, b's follower runs `grants::apply` on every change at the same
/// time as the test's own call, and the two rewrite the same records.
async fn idle() -> (Node, Node, Node) {
    let (http, _, _) = relay().await;
    let (a, b, c) = (
        Node::new(http, 1).await,
        Node::new(http, 2).await,
        Node::new(http, 3).await,
    );
    assert_eq!(a.ask(Ask::Found), Answer::Done);
    join(&a, &b);
    join(&a, &c);
    for node in [&b, &c] {
        node.fleet.receive(a.ops(), now_ms()).unwrap();
    }
    (a, b, c)
}

/// `node` with three computers in one fleet, founded by the first, all
/// listening on one relay.
async fn three() -> (Node, Node, Node) {
    let (http, _, _) = relay().await;
    let (mut a, mut b, mut c) = (
        Node::new(http, 1).await,
        Node::new(http, 2).await,
        Node::new(http, 3).await,
    );
    assert_eq!(a.ask(Ask::Found), Answer::Done);
    join(&a, &b);
    join(&a, &c);
    // Everyone holds everything, as sync over SSH would make it.
    for node in [&b, &c] {
        node.fleet.receive(a.ops(), now_ms()).unwrap();
    }
    for node in [&mut a, &mut b, &mut c] {
        node.start().await;
    }
    (a, b, c)
}
