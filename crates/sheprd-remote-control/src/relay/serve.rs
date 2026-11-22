//! One dial: the end-to-end tunnel, admitting only a paired device's
//! tunnel key, then TLS with the certificate for `127.0.0.1`, then the
//! API, as axum serves it on the network. A desktop let in over SSH
//! (`crate::ssh_access`) reaches the local SSH server instead.

use super::{Relays, Result, Served};
use crate::control::DeviceId;
use crate::web::{Peer, Relayed, Shared};
use axum::{Extension, extract::ConnectInfo};
use hyper_util::{rt::TokioIo, service::TowerToHyperService};
use sha2::{Digest, Sha256};
use sheprd_relay::{RouteId, client::Incoming};
use sheprd_tunnel::{Responder, stream};
use std::{
    net::{IpAddr, Ipv6Addr, SocketAddr},
    sync::Arc,
    time::Duration,
};
use tokio::io::join;

/// How long a dialer has to finish the handshake.
const HANDSHAKE: Duration = Duration::from_secs(10);

/// Serves one dial until either side closes. Failures end the session
/// alone; the log names no address or key.
pub(super) async fn session(
    relays: Arc<Relays>,
    incoming: Incoming,
    route: RouteId,
    served: Served,
) {
    if let Err(error) = serve(&relays, incoming, route, served).await {
        tracing::debug!(error = %error, "a relay session ended");
    }
}

async fn serve(relays: &Relays, incoming: Incoming, route: RouteId, served: Served) -> Result<()> {
    let Served { router, state, tls } = served;
    let state = &state;
    let tunnel_keys = &relays.keys.tunnel;
    // The open pairing dial runs the pairing handshake, whose PSK is the
    // token sealed to the nearby phone: any phone that holds it may claim,
    // as over the network. Every other dial must be a paired device's.
    let pairing = relays.pairing_secret(&incoming.key);
    let responder = match &pairing {
        Some(secret) => Responder::pairing(tunnel_keys, route.as_bytes(), secret)?,
        None => Responder::session(tunnel_keys, route.as_bytes())?,
    };
    let io = join(incoming.recv, incoming.send);
    let paired = |remote: &[u8; 32]| {
        pairing.is_some()
            || paired(state, remote)
            || state
                .fleet
                .as_ref()
                .is_some_and(|fleet| crate::fleet::grants::peer(fleet, remote).is_some())
    };
    let accepted = tokio::time::timeout(
        HANDSHAKE,
        stream::accept(io, tunnel_keys, responder, paired),
    )
    .await
    .map_err(|_| sheprd_tunnel::Error::Closed)??;
    // A paired phone's key reaches the API whatever a fleet bundle names:
    // keys are public, and a member must not be able to divert a phone's
    // sessions by naming its key.
    if pairing.is_none()
        && !paired_phone(state, &accepted.remote)
        && let Some(fleet) = &state.fleet
        && let Some(peer) = crate::fleet::grants::peer(fleet, &accepted.remote)
    {
        return fleet_session(
            state,
            fleet,
            peer,
            (&accepted.remote, &accepted.hash),
            accepted.io,
        )
        .await;
    }
    if pairing.is_none()
        && let Some((device, server)) = desktop(state, &accepted.remote)
    {
        return ssh(state, device, server, None, accepted.io).await;
    }
    // Rate limits and logs key callers by address; each device gets its own
    // private one, derived from its tunnel key, so one cannot spend
    // another's.
    let peer = SocketAddr::new(address_for(&accepted.remote), 0);
    // The marker tells a prompt's origin it came through the relay.
    let service = TowerToHyperService::new(
        router
            .layer(Extension(ConnectInfo(Peer(peer))))
            .layer(Extension(Relayed)),
    );
    let tls = tokio::time::timeout(
        HANDSHAKE,
        tokio_rustls::TlsAcceptor::from(tls).accept(accepted.io),
    )
    .await
    .map_err(|_| sheprd_tunnel::Error::Closed)?
    .map_err(|_| sheprd_tunnel::Error::Closed)?;
    let served = hyper::server::conn::http1::Builder::new()
        .serve_connection(TokioIo::new(tls), service)
        .with_upgrades()
        .await;
    if let Err(error) = served {
        tracing::debug!(error = %error, "a relay connection ended");
    }
    Ok(())
}

/// Whether `remote` is a paired device's tunnel key.
fn paired(state: &Shared, remote: &[u8; 32]) -> bool {
    state
        .auth()
        .devices()
        .records()
        .iter()
        .filter_map(|record| record.relay.as_ref()?.tunnel_key())
        .any(|key| key == *remote)
}

/// Whether `remote` is the tunnel key of a paired device that is not a
/// desktop let in over SSH: a phone.
fn paired_phone(state: &Shared, remote: &[u8; 32]) -> bool {
    state.auth().devices().records().iter().any(|record| {
        record.ssh.is_none()
            && record
                .relay
                .as_ref()
                .and_then(|relay| relay.tunnel_key())
                .is_some_and(|key| key == *remote)
    })
}

/// The desktop let in over SSH whose tunnel key `remote` is, and the SSH
/// server to reach.
fn desktop(state: &Shared, remote: &[u8; 32]) -> Option<(DeviceId, SocketAddr)> {
    let auth = state.auth();
    let server = auth.ssh_access().ok()?.server;
    auth.devices()
        .records()
        .iter()
        .find(|record| {
            record.ssh.is_some()
                && record
                    .relay
                    .as_ref()
                    .and_then(|relay| relay.tunnel_key())
                    .is_some_and(|key| key == *remote)
        })
        .map(|record| (record.id.clone(), server))
}

/// Splices a desktop's tunnel to the SSH server on loopback, whose own
/// encryption and host key run inside it, and marks the desktop seen so it
/// is not forgotten as idle while it uses only SSH. Revoking the desktop,
/// or its expiring, ends the session here at once, whatever the relay does
/// with its dial key.
async fn ssh(
    state: &Shared,
    device: DeviceId,
    server: SocketAddr,
    first: Option<u8>,
    mut tunnel: impl tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
) -> Result<()> {
    let mut revoked = state.auth().watch_revocations();
    let touched = state.clone();
    let seen = device.clone();
    let _ = tokio::task::spawn_blocking(move || {
        touched
            .auth()
            .devices_mut()
            .touch(&seen, crate::web::now_ms())
    })
    .await;
    let mut server = tokio::net::TcpStream::connect(server)
        .await
        .map_err(super::Error::Ssh)?;
    if let Some(first) = first {
        tokio::io::AsyncWriteExt::write_all(&mut server, &[first])
            .await
            .map_err(super::Error::Ssh)?;
    }
    let gone = async {
        loop {
            if state.auth().devices().get(&device).is_none() {
                return;
            }
            if revoked.changed().await.is_err() {
                // The daemon is shutting down; the copy ends with it.
                std::future::pending::<()>().await;
            }
        }
    };
    tokio::select! {
        copied = tokio::io::copy_bidirectional(&mut tunnel, &mut server) => {
            copied.map_err(super::Error::Ssh)?;
        }
        () = gone => tracing::info!("ended the SSH session of a desktop no longer paired"),
    }
    Ok(())
}

/// A fleet member's session: its first byte says what for, and a proof
/// that the device holds the member's fleet key follows it. A removal
/// exchange is served whether or not the member may reach SSH here; SSH
/// only when it may. Only a proven session ties the tunnel key to the
/// member: a bundle names keys that are public, so a member could name a
/// desktop paired here that is no member at all. A desktop paired here
/// over Bluetooth sends no purpose byte: its session is spliced as before,
/// as the member only once it proved that key in an earlier session.
async fn fleet_session(
    state: &Shared,
    fleet: &Arc<crate::fleet::Fleet>,
    peer: crate::fleet::grants::Peer,
    (remote, hash): (&[u8; 32], &[u8; 32]),
    mut tunnel: impl tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
) -> Result<()> {
    use crate::control::fleet::removal::{PROOF_LEN, PURPOSE_REMOVAL, PURPOSE_SSH, proves};
    let mut first = [0];
    tokio::io::AsyncReadExt::read_exact(&mut tunnel, &mut first)
        .await
        .map_err(super::Error::Ssh)?;
    if !matches!(first[0], PURPOSE_REMOVAL | PURPOSE_SSH) {
        // No purpose byte, so no proof: the member's only if an earlier
        // session proved this key for it.
        let proven = fleet
            .inner()
            .proven
            .get(&peer.member)
            .is_some_and(|keys| keys.contains(remote));
        if proven && peer.ssh {
            supersede(state, remote).await;
            return fleet_ssh(state, fleet, peer.member, Some(first[0]), tunnel).await;
        }
        return match desktop(state, remote) {
            Some((device, server)) => ssh(state, device, server, Some(first[0]), tunnel).await,
            None => Ok(()),
        };
    }
    let mut proof = [0; PROOF_LEN];
    tokio::io::AsyncReadExt::read_exact(&mut tunnel, &mut proof)
        .await
        .map_err(super::Error::Ssh)?;
    let proven = fleet
        .inner()
        .log
        .as_ref()
        .is_some_and(|log| proves(log.state(), &peer.member, hash, &proof));
    if !proven {
        tracing::info!("a fleet session did not prove its member's key; it ends");
        return Ok(());
    }
    // This device holds the member's tunnel key and its fleet key.
    let (proving, remote_key, member) = (fleet.clone(), *remote, peer.member);
    let kept = tokio::task::spawn_blocking(move || proving.proved(member, remote_key))
        .await
        .is_ok_and(|kept| matches!(kept, Ok(true)));
    if !kept {
        // Not remembered: its pairing cannot be left to outlive the
        // member's removal, so it goes now.
        supersede(state, remote).await;
    }
    match first[0] {
        PURPOSE_REMOVAL => {
            crate::fleet::removals::serve(fleet.clone(), &fleet.inbound, &mut tunnel).await;
            Ok(())
        }
        _ if peer.ssh => {
            supersede(state, remote).await;
            fleet_ssh(state, fleet, peer.member, None, tunnel).await
        }
        _ => Ok(()),
    }
}

/// Revokes a desktop paired here one by one with the tunnel key `remote`,
/// which the handshake just proved, as `fleet::grants::supersede` does.
async fn supersede(state: &Shared, remote: &[u8; 32]) {
    let (superseding, remote) = (state.clone(), *remote);
    let _ =
        tokio::task::spawn_blocking(move || crate::fleet::grants::supersede(&superseding, &remote))
            .await;
}

/// Splices a fleet member's tunnel to the SSH server on loopback, until
/// either side closes or the fleet no longer allows the member here.
async fn fleet_ssh(
    state: &Shared,
    fleet: &crate::fleet::Fleet,
    member: crate::control::fleet::MemberId,
    first: Option<u8>,
    mut tunnel: impl tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
) -> Result<()> {
    let server = state
        .auth()
        .ssh_access()
        .map(|access| access.server)
        .map_err(|_| super::Error::Ssh(std::io::ErrorKind::Unsupported.into()))?;
    let mut changed = fleet.watch();
    let mut server = tokio::net::TcpStream::connect(server)
        .await
        .map_err(super::Error::Ssh)?;
    // The client port the SSH server sees, as `fleet scrub` reports it.
    let client_port = server.local_addr().ok().map(|local| local.port());
    if let Some(first) = first {
        tokio::io::AsyncWriteExt::write_all(&mut server, &[first])
            .await
            .map_err(super::Error::Ssh)?;
    }
    let gone = async {
        loop {
            if !crate::fleet::grants::still_allowed(fleet, &member) {
                // A scrub asked over this session: let it read the answer.
                if let Some(left) = client_port.and_then(|port| fleet.scrub_grace(port)) {
                    tokio::time::sleep(left).await;
                }
                return;
            }
            if changed.changed().await.is_err() {
                std::future::pending::<()>().await;
            }
        }
    };
    tokio::select! {
        copied = tokio::io::copy_bidirectional(&mut tunnel, &mut server) => {
            copied.map_err(super::Error::Ssh)?;
        }
        () = gone => tracing::info!("ended the SSH session of a fleet member no longer allowed here"),
    }
    Ok(())
}

/// A unique local IPv6 address standing for the device with this tunnel
/// key: `fd`, then 15 bytes of its SHA-256.
fn address_for(remote: &[u8; 32]) -> IpAddr {
    let digest = Sha256::digest(remote);
    let mut bytes = [0; 16];
    bytes[0] = 0xfd;
    bytes[1..].copy_from_slice(&digest[..15]);
    IpAddr::V6(Ipv6Addr::from(bytes))
}
