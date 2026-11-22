//! Removals passed between daemons, and to plain hosts' agents, inside the
//! relay tunnel (`docs/relay-plan.md`, Removals between daemons;
//! `control::fleet::removal` for the wire).
//!
//! A daemon that holds a removal some live computer member has not said
//! it holds dials that member's route with its own desktop keys, opens the
//! tunnel with the removal purpose, and sends it, with any ancestors asked
//! for, until the member answers that it holds it. Each daemon that takes
//! one does the same, so a removal floods the fleet in about two hops,
//! with no Sheprd running anywhere.
//!
//! Plain hosts get the removals that took effect the same way, each from
//! the member that signed it: their agent checks no signature, so it takes
//! a removal only in the dialing member's own name, only suspends the
//! targets until Sheprd's next access file settles it, and answers `Held`
//! at once.
//!
//! A daemon taking a removal whose history has not arrived suspends its
//! targets at once (`Fleet::suspended`), then asks for the history.

use super::{Fleet, Holder};
use crate::control::fleet::{
    Body, HostKind, Log, MemberId, Op, OpId, State, access,
    removal::{MAX_FRAME, MAX_OPS, MAX_ROUNDS, Message, PURPOSE_REMOVAL},
};
use crate::desktop::record::Record;
use std::{
    collections::BTreeMap,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt},
    sync::Semaphore,
};

pub(crate) use crate::control::fleet::removal::{CONCURRENT, SESSION};
const FIRST_RETRY: Duration = Duration::from_secs(5);
const LAST_RETRY: Duration = Duration::from_secs(10 * 60);

/// Serves one removal exchange a fleet member dialed in for: takes its
/// removals, asks for what history is missing, and says what it holds.
pub(crate) async fn serve(
    fleet: Arc<Fleet>,
    inbound: &Semaphore,
    io: &mut (impl AsyncRead + AsyncWrite + Unpin),
) {
    let Ok(_permit) = inbound.try_acquire() else {
        return;
    };
    if tokio::time::timeout(SESSION, exchange_in(&fleet, io))
        .await
        .is_err()
    {
        tracing::debug!("a fleet removal exchange timed out");
    }
}

async fn exchange_in(fleet: &Arc<Fleet>, io: &mut (impl AsyncRead + AsyncWrite + Unpin)) {
    // The fleet this exchange serves: what it passes goes into that one
    // only, never into a fleet joined since, or none after a scrub.
    let Some(serving) = fleet.inner().log.as_ref().map(Log::fleet) else {
        return;
    };
    let mut received: Vec<OpId> = Vec::new();
    for round in 0..=MAX_ROUNDS {
        let Ok(Message::Removal(ops)) = read(io).await else {
            return;
        };
        let Ok(ops) = ops
            .iter()
            .map(|op| Op::decode(op))
            .collect::<Result<Vec<_>, _>>()
        else {
            return;
        };
        received.extend(
            ops.iter()
                .filter(|op| matches!(op.body(), Body::Remove(_)))
                .map(Op::id),
        );
        let taken = fleet.clone();
        // A removal not saved is not acknowledged: the dialer keeps it and
        // sends it again, and that later exchange saves it.
        let Ok(Some(wanted)) = tokio::task::spawn_blocking(move || {
            if let Err(error) = taken.receive_in(serving, ops, crate::web::now_ms()) {
                tracing::warn!(error = %error, "could not keep removals another member passed");
                return None;
            }
            Some(
                taken
                    .inner()
                    .log
                    .as_ref()
                    .map(|log| log.wanted())
                    .unwrap_or_default(),
            )
        })
        .await
        else {
            return;
        };
        if wanted.is_empty() || round == MAX_ROUNDS {
            break;
        }
        let wanted = wanted.into_iter().take(MAX_OPS).collect();
        if write(io, &Message::Want(wanted)).await.is_err() {
            return;
        }
    }
    let held: Vec<OpId> = {
        let inner = fleet.inner();
        received
            .into_iter()
            .filter(|id| inner.log.as_ref().is_some_and(|log| log.get(id).is_some()))
            .take(MAX_OPS)
            .collect()
    };
    let _ = write(io, &Message::Held(held)).await;
}

/// Each peer's next try, with its delay so far.
#[derive(Debug, Default)]
struct Backoff {
    next: BTreeMap<Holder, (Instant, Duration)>,
}

impl Backoff {
    fn due(&self, peer: &Holder, now: Instant) -> bool {
        self.next.get(peer).is_none_or(|(at, _)| *at <= now)
    }

    fn failed(&mut self, peer: Holder, now: Instant) {
        let delay = self
            .next
            .get(&peer)
            .map_or(FIRST_RETRY, |(_, delay)| (*delay * 2).min(LAST_RETRY));
        // Up to a quarter more, so peers that failed together do not retry
        // together.
        let jitter = delay.mul_f64(f64::from(rand_quarter()) / 4.0);
        self.next.insert(peer, (now + delay + jitter, delay));
    }

    fn done(&mut self, peer: &Holder) {
        self.next.remove(peer);
    }
}

fn rand_quarter() -> u8 {
    let mut byte = [0];
    let _ = getrandom::fill(&mut byte);
    byte[0] % 2
}

/// What one peer is owed: the record to dial it at, the removals, and for
/// a plain host's agent, the members the log removes with them that they
/// do not name ([`Message::Also`]).
pub(super) type Owed = (Holder, Record, Vec<Op>, Vec<(OpId, MemberId)>);

/// What each live computer member and plain host is owed: the removals
/// the log holds that it has not said it holds, and the record to dial it
/// at. A plain host is owed only the removals that took effect, since its
/// agent acts on what it is sent without checking it, and with each the
/// members the log removes with it beyond those it names: a member's
/// daemon works those out from the log, an agent cannot.
pub(super) fn owed(fleet: &Fleet) -> Vec<Owed> {
    let inner = fleet.inner();
    let Some(log) = &inner.log else {
        return Vec::new();
    };
    let removals: Vec<&Op> = log
        .ops()
        .filter(|op| matches!(op.body(), Body::Remove(_)))
        .collect();
    if removals.is_empty() {
        return Vec::new();
    }
    let state = log.state();
    let unheld = |holder: &Holder, ops: &mut dyn Iterator<Item = &&Op>| -> Vec<Op> {
        let held = inner.held.get(holder);
        ops.filter(|op| held.is_none_or(|held| !held.contains(&op.id())))
            .take(MAX_OPS)
            .map(|op| (*op).clone())
            .collect()
    };
    let members = state
        .members
        .iter()
        .filter(|(id, member)| **id != fleet.me() && member.bundle.host.is_some())
        .filter_map(|(id, member)| {
            let host = member.bundle.host.as_ref()?.id().ok()?;
            let record = record(state, &host, &member.bundle.name)?;
            let holder = Holder::Member(*id);
            let ops = unheld(&holder, &mut removals.iter());
            Some((holder, record, ops, Vec::new()))
        });
    // What each removal in effect removes; none when the log cannot say,
    // and the access file then carries it.
    let removed_by = log.removed_by().unwrap_or_default();
    let hosts = state
        .hosts
        .iter()
        // A held host's agent too: removals reach it while its access is
        // paused.
        .filter(|(_, host)| host.intro.kind == HostKind::Agent)
        .filter_map(|(id, host)| {
            let record = record(state, id, &host.intro.name)?;
            let holder = Holder::Host(*id);
            // An agent suspends only in the dialing member's own name, so
            // it is passed only this computer's removals; the others reach
            // it in the access file.
            let effective: Vec<&Op> = removals
                .iter()
                .copied()
                .filter(|op| op.signer() == fleet.me() && access::for_agents(state, op))
                .collect();
            // A removal the agent holds is owed again once it takes a
            // member the agent was not told of: one admitted with a
            // removed member, whose admission arrived after.
            let took = inner.held_also.get(id);
            let grew: Vec<OpId> = access::also(
                &removed_by,
                &effective.iter().map(|op| (*op).clone()).collect::<Vec<_>>(),
            )
            .into_iter()
            .filter(|(removal, member)| {
                !took
                    .and_then(|took| took.get(removal))
                    .is_some_and(|took| took.contains(member))
            })
            .map(|(removal, _)| removal)
            .collect();
            let held = inner.held.get(&holder);
            let ops: Vec<Op> = effective
                .iter()
                .filter(|op| {
                    held.is_none_or(|held| !held.contains(&op.id())) || grew.contains(&op.id())
                })
                .take(MAX_OPS)
                .map(|op| (*op).clone())
                .collect();
            let also = access::also(&removed_by, &ops);
            Some((holder, record, ops, also))
        });
    members
        .chain(hosts)
        .filter(|(_, _, ops, _)| !ops.is_empty())
        .collect()
}

/// The record to dial host `host` at, from its signed self-record.
fn record(state: &State, host: &crate::control::fleet::HostId, name: &str) -> Option<Record> {
    let record = &state.hosts.get(host)?.record;
    Some(Record {
        computer: name.to_owned(),
        identity: record.identity.clone(),
        relays: record.relays.clone(),
        route: record.route.clone(),
        fleet: true,
        host: None,
        // Members and plain hosts' agents both read the purpose byte and
        // proof first.
        opening: true,
    })
}

/// Passes removals to every live computer member and plain host that has
/// not said it holds them, for the life of the daemon.
pub(crate) async fn flood(fleet: Arc<Fleet>) {
    let mut changed = fleet.watch();
    let mut backoff = Backoff::default();
    let outbound = Arc::new(Semaphore::new(CONCURRENT));
    let (done_send, mut done) = tokio::sync::mpsc::channel::<(Holder, bool)>(64);
    let mut busy = std::collections::BTreeSet::new();
    let mut tick = tokio::time::interval(FIRST_RETRY);
    loop {
        tokio::select! {
            result = changed.changed() => if result.is_err() { return },
            _ = tick.tick() => {}
            Some((peer, ok)) = done.recv() => {
                busy.remove(&peer);
                if ok { backoff.done(&peer) } else { backoff.failed(peer, Instant::now()) }
                continue;
            }
        }
        let now = Instant::now();
        let owed = {
            let fleet = fleet.clone();
            tokio::task::spawn_blocking(move || owed(&fleet))
                .await
                .unwrap_or_default()
        };
        for (peer, record, ops, also) in owed {
            if busy.contains(&peer) || !backoff.due(&peer, now) {
                continue;
            }
            let Ok(permit) = outbound.clone().try_acquire_owned() else {
                break;
            };
            busy.insert(peer);
            let (fleet, done) = (fleet.clone(), done_send.clone());
            tokio::spawn(async move {
                let ok = tokio::time::timeout(SESSION, push(&fleet, peer, &record, ops, also))
                    .await
                    .is_ok_and(|ok| ok);
                drop(permit);
                let _ = done.send((peer, ok)).await;
            });
        }
    }
}

/// One exchange with `peer`: whether it ended with the peer holding them.
async fn push(
    fleet: &Arc<Fleet>,
    peer: Holder,
    record: &Record,
    ops: Vec<Op>,
    also: Vec<(OpId, MemberId)>,
) -> bool {
    let state_dir = fleet.state_dir().to_owned();
    let Ok(Ok(keys)) =
        tokio::task::spawn_blocking(move || crate::desktop::keys::Keys::load_or_create(&state_dir))
            .await
    else {
        return false;
    };
    let Ok((connection, mut tunnel)) = crate::desktop::connect::open_record(record, &keys).await
    else {
        return false;
    };
    let mut opening = vec![PURPOSE_REMOVAL];
    opening.extend_from_slice(&crate::control::fleet::removal::prove(
        fleet.key(),
        &tunnel.hash,
    ));
    let io = &mut tunnel.io;
    if io.write_all(&opening).await.is_err() {
        return false;
    }
    // Ahead of the removals, for an agent: who they take out beyond those
    // they name.
    if !also.is_empty() {
        let pairs = also.iter().map(|(id, member)| (*id, member.0)).collect();
        if write(io, &Message::Also(pairs)).await.is_err() {
            return false;
        }
    }
    let sent = Message::Removal(ops.iter().map(Op::encode).collect());
    if write(io, &sent).await.is_err() {
        return false;
    }
    for _ in 0..=MAX_ROUNDS {
        match read(io).await {
            Ok(Message::Want(ids)) => {
                let wanted: Vec<Vec<u8>> = {
                    let inner = fleet.inner();
                    let Some(log) = &inner.log else {
                        return false;
                    };
                    ids.iter()
                        .filter_map(|id| log.get(id))
                        .map(Op::encode)
                        .collect()
                };
                if write(io, &Message::Removal(wanted)).await.is_err() {
                    return false;
                }
            }
            Ok(Message::Held(ids)) => {
                let ids = acknowledged(&ops, ids);
                let all = ops.iter().all(|op| ids.contains(&op.id()));
                let fleet = fleet.clone();
                let _ = tokio::task::spawn_blocking(move || fleet.held(peer, ids, also)).await;
                drop(connection);
                return all;
            }
            _ => return false,
        }
    }
    false
}

/// Of the ids a peer says it holds, those sent to it in this exchange:
/// a peer cannot fill what is kept for it with ids it makes up.
pub(crate) fn acknowledged(sent: &[Op], held: Vec<OpId>) -> Vec<OpId> {
    let sent: std::collections::BTreeSet<OpId> = sent.iter().map(Op::id).collect();
    held.into_iter().filter(|id| sent.contains(id)).collect()
}

async fn read(io: &mut (impl AsyncRead + Unpin)) -> crate::control::fleet::Result<Message> {
    let mut length = [0; 4];
    io.read_exact(&mut length)
        .await
        .map_err(|_| crate::control::fleet::Error::Encoding)?;
    let length = usize::try_from(u32::from_be_bytes(length))
        .map_err(|_| crate::control::fleet::Error::Bounds)?;
    if length > MAX_FRAME {
        return Err(crate::control::fleet::Error::Bounds);
    }
    let mut bytes = vec![0; length];
    io.read_exact(&mut bytes)
        .await
        .map_err(|_| crate::control::fleet::Error::Encoding)?;
    Ok(Message::decode(&bytes)?)
}

async fn write(
    io: &mut (impl AsyncWrite + Unpin),
    message: &Message,
) -> crate::control::fleet::Result<()> {
    let bytes = message.encode()?;
    let length = u32::try_from(bytes.len()).map_err(|_| crate::control::fleet::Error::Bounds)?;
    io.write_all(&length.to_be_bytes())
        .await
        .map_err(|_| crate::control::fleet::Error::Encoding)?;
    io.write_all(&bytes)
        .await
        .map_err(|_| crate::control::fleet::Error::Encoding)?;
    io.flush()
        .await
        .map_err(|_| crate::control::fleet::Error::Encoding)
}
