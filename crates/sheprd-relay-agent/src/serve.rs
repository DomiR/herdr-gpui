//! One dial: the end-to-end tunnel, admitting only a desktop's or a fleet
//! member's tunnel key, then what the session is for.
//!
//! A desktop enrolled one by one is spliced to the SSH server at once. A
//! fleet member's session starts with a purpose byte (`removal`): SSH,
//! spliced only when the member may reach SSH here, or a removal
//! exchange, which suspends its targets (`fleet::Held::suspend`). A member
//! that is also an enrolled desktop and sends no purpose byte is spliced
//! as that desktop, its first byte included. Every splice ends when who
//! is admitted no longer lets it go on.

use crate::{
    Error, Result,
    agent::Agent,
    fleet::{Held, Peer},
    removal::{self, MAX_OPS, Message, PROOF_LEN, PURPOSE_REMOVAL, PURPOSE_SSH},
};
use sheprd_relay::{RouteId, client::Pending};
use sheprd_tunnel::{Responder, stream};
use std::{sync::Arc, time::Duration};
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, copy_bidirectional, join},
    net::TcpStream,
};

/// How long a dialer has to finish the handshake, and to say what for.
const HANDSHAKE: Duration = Duration::from_secs(10);

/// Serves one dial until it ends. Failures end the session alone; the log
/// names no address or key.
pub(crate) async fn session(agent: Arc<Agent>, pending: Pending, route: RouteId) {
    if let Err(error) = serve(&agent, pending, route).await {
        tracing::debug!(error = %error, "a relay session ended");
    }
}

async fn serve(agent: &Arc<Agent>, pending: Pending, route: RouteId) -> Result<()> {
    let incoming = tokio::time::timeout(HANDSHAKE, pending.incoming())
        .await
        .map_err(|_| sheprd_tunnel::Error::Closed)??;
    let tunnel_keys = &agent.keys.tunnel;
    let responder = Responder::session(tunnel_keys, route.as_bytes())?;
    let admitted = agent.admitted();
    let dial = incoming.key;
    let io = join(incoming.recv, incoming.send);
    let accepted = {
        let admits = |remote: &[u8; 32]| admitted.borrow().peer(&dial, remote).is_some();
        tokio::time::timeout(
            HANDSHAKE,
            stream::accept(io, tunnel_keys, responder, admits),
        )
        .await
        .map_err(|_| sheprd_tunnel::Error::Closed)??
    };
    let (remote, hash) = (accepted.remote, accepted.hash);
    let Some(peer) = admitted.borrow().peer(&dial, &remote) else {
        return Ok(());
    };
    let mut tunnel = accepted.io;
    let Peer::Member { id, ssh, desktop } = peer else {
        return splice(agent, tunnel, Peer::Desktop, dial, remote, None).await;
    };
    let mut first = [0];
    tokio::time::timeout(HANDSHAKE, tunnel.read_exact(&mut first))
        .await
        .map_err(|_| sheprd_tunnel::Error::Closed)?
        .map_err(Error::Splice)?;
    if matches!(first[0], PURPOSE_REMOVAL | PURPOSE_SSH) {
        // The member's fleet key signs this session's handshake: without
        // it, the session is only a device holding keys a bundle names.
        let mut proof = [0; PROOF_LEN];
        tokio::time::timeout(HANDSHAKE, tunnel.read_exact(&mut proof))
            .await
            .map_err(|_| sheprd_tunnel::Error::Closed)?
            .map_err(Error::Splice)?;
        if !admitted.borrow().proves(&id, &hash, &proof) {
            tracing::info!("ended a fleet session whose proof did not check out");
            return Ok(());
        }
    }
    match first[0] {
        PURPOSE_REMOVAL => removals(agent, id, &mut tunnel).await,
        PURPOSE_SSH if ssh => splice(agent, tunnel, peer, dial, remote, None).await,
        PURPOSE_SSH => Ok(()),
        byte if desktop => splice(agent, tunnel, Peer::Desktop, dial, remote, Some(byte)).await,
        _ => Ok(()),
    }
}

/// Splices the tunnel to the SSH server, `first` sent ahead when it was
/// read already, until either side closes or `peer` may no longer reach
/// SSH here.
async fn splice(
    agent: &Agent,
    mut tunnel: impl AsyncRead + AsyncWrite + Unpin,
    peer: Peer,
    dial: sheprd_relay::wire::Key,
    remote: [u8; 32],
    first: Option<u8>,
) -> Result<()> {
    let mut admitted = agent.admitted();
    let mut ssh = TcpStream::connect(agent.forward())
        .await
        .map_err(Error::Forward)?;
    if let Some(byte) = first {
        ssh.write_all(&[byte]).await.map_err(Error::Splice)?;
    }
    tokio::select! {
        copied = copy_bidirectional(&mut tunnel, &mut ssh) => {
            copied.map_err(Error::Splice)?;
        }
        // The relay also ends the session when the dial key goes; this
        // covers a key removed under another, a deny, and a suspension.
        _ = admitted.wait_for(|admitted| !admitted.keeps_ssh(&peer, &dial, &remote)) => {
            tracing::info!("ended a session no longer admitted");
        }
    }
    Ok(())
}

/// One removal exchange from member `sender`: at most
/// `removal::CONCURRENT` at once, each within `removal::SESSION`.
async fn removals(
    agent: &Arc<Agent>,
    sender: crate::access::Id,
    io: &mut (impl AsyncRead + AsyncWrite + Unpin),
) -> Result<()> {
    let Ok(_permit) = agent.removals.try_acquire() else {
        return Ok(());
    };
    match tokio::time::timeout(removal::SESSION, exchange(agent, sender, io)).await {
        Ok(result) => result,
        Err(_) => {
            tracing::debug!("a fleet removal exchange timed out");
            Ok(())
        }
    }
}

/// Reads the removals, and the members the sender's log removes with
/// them when it says, suspends their targets, reads who is admitted
/// again, and answers with the removals taken. Nothing is checked but the
/// shape: the next access file settles what this did.
async fn exchange(
    agent: &Arc<Agent>,
    sender: crate::access::Id,
    io: &mut (impl AsyncRead + AsyncWrite + Unpin),
) -> Result<()> {
    let (also, first) = match read(io).await? {
        Message::Also(pairs) => (pairs, read(io).await?),
        first => (Vec::new(), first),
    };
    let Message::Removal(ops) = first else {
        return Ok(());
    };
    let removed: Vec<_> = ops
        .iter()
        .filter_map(|op| removal::peek(op).ok().flatten())
        .map(|removal| with_also(removal, &also))
        .collect();
    let blocking = Arc::clone(agent);
    let suspended = tokio::task::spawn_blocking(move || {
        let taken = Held::suspend(
            blocking.state_dir(),
            blocking.authorized_keys(),
            sender,
            &removed,
        )?;
        blocking.reload()?;
        Ok::<_, Error>(taken)
    })
    .await;
    let Ok(taken) = suspended else {
        return Ok(());
    };
    let taken: Vec<[u8; 32]> = taken?.into_iter().take(MAX_OPS).collect();
    if !taken.is_empty() {
        tracing::info!(removals = taken.len(), "took fleet removals from a member");
    }
    io.write_all(&Message::Held(taken).frame()?)
        .await
        .map_err(Error::Splice)?;
    io.flush().await.map_err(Error::Splice)
}

/// `removal` with the members `also` says the sender's log removes with
/// it as targets too: suspended in the sender's name alone, like those it
/// names, and settled with it by the next access file.
fn with_also(mut removal: removal::Removed, also: &[([u8; 32], [u8; 32])]) -> removal::Removed {
    for (_, member) in also.iter().filter(|(id, _)| *id == removal.id) {
        if !removal.targets.contains(member) {
            removal.targets.push(*member);
        }
    }
    removal
}

async fn read(io: &mut (impl AsyncRead + Unpin)) -> Result<Message<[u8; 32]>> {
    let mut length = [0; 4];
    io.read_exact(&mut length).await.map_err(Error::Splice)?;
    let mut bytes = vec![0; removal::frame_length(length)?];
    io.read_exact(&mut bytes).await.map_err(Error::Splice)?;
    Ok(Message::decode(&bytes)?)
}
