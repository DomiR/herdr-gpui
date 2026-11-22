//! The fleet's files, in the state directory's `fleet` folder, private to
//! this user: `key` (the fleet key's 32-byte secret), `log.bin` (the log,
//! `control::fleet::saved`), and `peers.json` (which removals each member
//! and plain host holds, and which suspensions wait for their history);
//! `joining/` (the history of each fleet being joined, `FLEET.bin` in
//! `log.bin`'s format, until one admits this computer or shares it as a
//! host); `hosted` (the fleet it joined as a shared host); `lock` is
//! `super::lock`'s.

use super::{
    Error, Holder, Inner, Result, Suspensions,
    joining::{Joining, MAX_FLEETS},
};
use crate::control::fleet::{FleetId, HostId, MemberId, Op, OpId, saved, sync};
use crate::desktop::files::{read_bounded, write_private};
use p256::ecdsa::SigningKey;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

pub(super) const DIRECTORY: &str = "fleet";
const KEY: &str = "key";
const LOG: &str = "log.bin";
/// The history of each fleet being joined, until one admits this computer.
const JOINING: &str = "joining";
const PEERS: &str = "peers.json";
/// The fleet id this computer joined as a host the fleet shares, not as a
/// member.
const HOSTED: &str = "hosted";
/// At worst, suspensions take about 23.5 MB of it (a save's first write
/// keeping what the file held beside what is held, each folded target
/// still within its bound), the removals kept to check again about 6 MB,
/// and what each peer holds about 2 MB: `tests/folding.rs` checks the sum
/// stays under it.
pub(super) const MAX_PEERS: u64 = 32 * 1024 * 1024;

fn path(state_dir: &Path, name: &str) -> PathBuf {
    state_dir.join(DIRECTORY).join(name)
}

/// The fleet key, made and kept the first time.
pub(super) fn load_or_create_key(state_dir: &Path) -> crate::desktop::Result<SigningKey> {
    let file = path(state_dir, KEY);
    if let Some(bytes) = read_bounded(&file, 32)? {
        return SigningKey::from_slice(&bytes).map_err(|_| crate::desktop::Error::RecordFormat);
    }
    let mut secret = [0; 32];
    // A scalar out of range is drawn again; vanishingly rare.
    let key = loop {
        getrandom::fill(&mut secret).map_err(|_| crate::desktop::Error::RecordFormat)?;
        if let Ok(key) = SigningKey::from_slice(&secret) {
            break key;
        }
    };
    write_private(&file, &secret)?;
    Ok(key)
}

#[derive(Default, Serialize, Deserialize)]
struct Peers {
    /// By member.
    #[serde(default)]
    held: BTreeMap<String, BTreeSet<String>>,
    /// By plain host, whose agent holds them.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    held_hosts: BTreeMap<String, BTreeSet<String>>,
    /// By plain host, then removal: the members beyond those it names that
    /// the host's agent took with it.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    held_also: BTreeMap<String, BTreeMap<String, BTreeSet<String>>>,
    #[serde(default)]
    unconfirmed: BTreeMap<String, Kept>,
    /// Suspensions folded per signer and target (`super::suspensions`),
    /// hex, by signer then target.
    #[serde(default)]
    folds: BTreeMap<String, BTreeMap<String, FoldedKept>>,
    /// Folded suspensions as the first build to fold wrote them: read,
    /// never written.
    #[serde(default, skip_serializing)]
    folded: Vec<FoldedFirst>,
    #[serde(default)]
    proven: BTreeMap<String, Proven>,
    /// What recorded `proven`: [`PROVEN_VERSION`] once keys are recorded
    /// only for a session that proved the member's fleet key. Absent, as
    /// from earlier builds, a tunnel handshake alone recorded them.
    #[serde(default)]
    proven_version: u32,
    /// Removals plain hosts reported that the log has not heard of.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    unknown: BTreeMap<String, super::plain::Unknown>,
}

/// `proven` holds only keys a session proved with the member's fleet key
/// (`control::fleet::removal`). Earlier builds recorded a key on the
/// tunnel handshake alone, which any member's bundle could name, so their
/// entries are dropped on load: a key counts as a member's only once a
/// proven session records it again.
const PROVEN_VERSION: u32 = 2;

/// A member's proven keys: one, as an earlier build kept it, or several.
#[derive(Serialize, Deserialize)]
#[serde(untagged)]
enum Proven {
    Many(BTreeSet<String>),
    One(String),
}

impl Proven {
    fn keys(&self) -> impl Iterator<Item = &String> {
        let (many, one) = match self {
            Self::Many(keys) => (Some(keys), None),
            Self::One(key) => (None, Some(key)),
        };
        many.into_iter().flatten().chain(one)
    }
}

#[derive(Default, Serialize, Deserialize)]
struct FoldedKept {
    /// Each removal kept by id, hex.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    ids: BTreeMap<String, IdKept>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    overflow: Option<OverflowKept>,
    // What earlier builds wrote in place of `ids` and `overflow`: read,
    // never written, and read into them failing closed.
    #[serde(default, skip_serializing)]
    overflowed: Option<u64>,
    #[serde(default, skip_serializing)]
    named_with: Vec<String>,
    #[serde(default, skip_serializing)]
    pinned: Option<PinnedKept>,
    #[serde(default, skip_serializing)]
    since_ms: Option<u64>,
    #[serde(default, skip_serializing)]
    named: BTreeMap<String, u64>,
}

/// A removal kept by id: whole, or, as earlier builds wrote it, only when
/// it arrived.
#[derive(Serialize, Deserialize)]
#[serde(untagged)]
enum IdKept {
    Removal(RemovalKept),
    Since(u64),
}

/// An earlier build's pin: a flag, or when the removal it rests on
/// arrived.
#[derive(Serialize, Deserialize)]
#[serde(untagged)]
enum PinnedKept {
    Flag(bool),
    Since(u64),
}

/// A folded suspension as the first build to fold wrote it.
#[derive(Deserialize)]
struct FoldedFirst {
    signer: String,
    target: String,
    #[serde(default)]
    ids: BTreeMap<String, u64>,
    #[serde(default)]
    overflowed: Option<u64>,
}

#[derive(Serialize, Deserialize)]
struct RemovalKept {
    since_ms: u64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    named: Vec<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pinned: bool,
}

#[derive(Serialize, Deserialize)]
struct OverflowKept {
    since_ms: u64,
    /// Each member named, hex, with when the latest removal naming it
    /// arrived.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    named: BTreeMap<String, u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pinned: Option<u64>,
}

#[derive(Serialize, Deserialize)]
struct Kept {
    signer: String,
    targets: Vec<String>,
    since_ms: u64,
    /// The removal, base64url; absent from files an earlier build wrote.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    op: Option<String>,
}

/// What the fleet folder keeps, read at `now_ms`. A log that no longer
/// reads is an error rather than an empty fleet, so a damaged file never
/// looks like having left.
pub(super) fn load(state_dir: &Path, now_ms: u64) -> Result<Inner> {
    let log = read_bounded(&path(state_dir, LOG), saved::MAX_SAVED as u64)?
        .map(|bytes| saved::decode(&bytes, now_ms))
        .transpose()?;
    // A file that no longer reads is an error, never an empty one: it may
    // hold suspensions, and dropping them would let their targets back in.
    let peers = read_peers(state_dir)?;
    let joining = match log {
        Some(_) => BTreeMap::new(),
        None => load_joining(state_dir, now_ms)?,
    };
    Ok(Inner {
        joining,
        log,
        held: peers
            .held
            .iter()
            .filter_map(|(peer, ids)| Some((Holder::Member(MemberId::from_hex(peer)?), ids)))
            .chain(
                peers
                    .held_hosts
                    .iter()
                    .filter_map(|(host, ids)| Some((Holder::Host(HostId::from_hex(host)?), ids))),
            )
            .map(|(holder, ids)| {
                // Forgetting one only sends it again.
                let ids = ids
                    .iter()
                    .filter_map(|id| OpId::from_hex(id))
                    .take(super::MAX_HELD_PER_PEER)
                    .collect();
                (holder, ids)
            })
            .collect(),
        // One that no longer reads is only sent again.
        held_also: peers
            .held_also
            .iter()
            .filter_map(|(host, kept)| {
                let kept = kept
                    .iter()
                    .filter_map(|(id, members)| {
                        let members = members
                            .iter()
                            .filter_map(|member| MemberId::from_hex(member))
                            .collect();
                        Some((OpId::from_hex(id)?, members))
                    })
                    .collect();
                Some((HostId::from_hex(host)?, kept))
            })
            .collect(),
        // Keys an earlier build recorded on a handshake alone do not count,
        // here or in a scrub (`proven_keys`).
        proven: counted(&peers),
        unknown: peers
            .unknown
            .iter()
            .filter_map(|(id, seen)| Some((OpId::from_hex(id)?, *seen)))
            .collect(),
        folded: folded(&peers.folds, &peers.folded)?,
        folded_pending: BTreeSet::new(),
        unconfirmed: unconfirmed(&peers.unconfirmed)?,
    })
}

fn read_peers(state_dir: &Path) -> Result<Peers> {
    Ok(read_bounded(&path(state_dir, PEERS), MAX_PEERS)?
        .map(|bytes| serde_json::from_slice(&bytes))
        .transpose()
        .map_err(|_| Error::Peers)?
        .unwrap_or_default())
}

fn proven(peers: &Peers) -> BTreeMap<MemberId, BTreeSet<[u8; 32]>> {
    peers
        .proven
        .iter()
        .filter_map(|(member, keys)| {
            let keys = keys
                .keys()
                .filter_map(|key| HostId::from_hex(key).map(|key| key.0))
                .collect();
            Some((MemberId::from_hex(member)?, keys))
        })
        .collect()
}

/// The proven keys that count: those a proven session recorded. An
/// earlier build's, recorded on a handshake alone, could name any
/// desktop's key, so neither a removal nor a scrub revokes a pairing for
/// them.
fn counted(peers: &Peers) -> BTreeMap<MemberId, BTreeSet<[u8; 32]>> {
    if peers.proven_version == PROVEN_VERSION {
        proven(peers)
    } else {
        BTreeMap::new()
    }
}

/// Every tunnel key a member proved, read from the peers' state alone, so
/// a damaged log does not stop a scrub from revoking their pairings.
/// Blocking.
pub(super) fn proven_keys(state_dir: &Path) -> Result<Vec<[u8; 32]>> {
    Ok(counted(&read_peers(state_dir)?)
        .into_values()
        .flatten()
        .collect())
}

/// The fleet id this computer joined as a shared host, if it did. Blocking.
pub(super) fn load_hosted(state_dir: &Path) -> Result<Option<String>> {
    Ok(read_bounded(&path(state_dir, HOSTED), 64)?
        .map(|bytes| String::from_utf8_lossy(&bytes).into_owned()))
}

/// Records that this computer joined `fleet` as a shared host. Blocking.
pub(super) fn save_hosted(state_dir: &Path, fleet: &str) -> Result<()> {
    write_private(&path(state_dir, HOSTED), fleet.as_bytes())?;
    Ok(())
}

/// Removes the log and how this computer joined it, once it left the
/// fleet for good; `save` then writes the peers' state empty. Blocking.
pub(super) fn forget(state_dir: &Path) -> Result<()> {
    remove_joining(state_dir)?;
    for name in [LOG, HOSTED] {
        let file = path(state_dir, name);
        match std::fs::remove_file(&file) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(source) => {
                return Err(crate::desktop::Error::File {
                    operation: "remove",
                    path: file,
                    source,
                }
                .into());
            }
        }
    }
    Ok(())
}

/// Hex that no longer reads in a suspension is an error, never a
/// suspension dropped: it would let its targets back in.
fn member(hex: &str) -> Result<MemberId> {
    MemberId::from_hex(hex).ok_or(Error::Peers)
}

fn folded(
    kept: &BTreeMap<String, BTreeMap<String, FoldedKept>>,
    first: &[FoldedFirst],
) -> Result<super::suspensions::FoldedMap> {
    let mut folded = super::suspensions::FoldedMap::new();
    for (signer, targets) in kept {
        let into = folded.entry(member(signer)?).or_default();
        for (target, kept) in targets {
            into.insert(member(target)?, folded_target(kept)?);
        }
    }
    for kept in first {
        let earlier = FoldedKept {
            ids: kept
                .ids
                .iter()
                .map(|(id, since)| (id.clone(), IdKept::Since(*since)))
                .collect(),
            overflow: None,
            overflowed: kept.overflowed,
            named_with: Vec::new(),
            pinned: None,
            since_ms: None,
            named: BTreeMap::new(),
        };
        folded
            .entry(member(&kept.signer)?)
            .or_default()
            .insert(member(&kept.target)?, folded_target(&earlier)?);
    }
    Ok(folded)
}

/// One folded target as saved, in this build's shape or an earlier one.
/// What an earlier shape cannot say (which members a removal named) is
/// read so that the suspension holds until its time: a removal kept by
/// arrival alone is pinned, and an earlier overflow or folded target
/// becomes an overflow pinned to its latest arrival.
fn folded_target(kept: &FoldedKept) -> Result<super::suspensions::FoldedTarget> {
    use super::suspensions::{FoldedRemoval, FoldedTarget, Overflow};
    let mut target = FoldedTarget {
        ids: kept
            .ids
            .iter()
            .map(|(id, removal)| {
                let id = OpId::from_hex(id).ok_or(Error::Peers)?;
                let removal = match removal {
                    IdKept::Removal(removal) => FoldedRemoval {
                        since_ms: removal.since_ms,
                        named: removal
                            .named
                            .iter()
                            .map(|hex| member(hex))
                            .collect::<Result<_>>()?,
                        pinned: removal.pinned,
                    },
                    IdKept::Since(since_ms) => FoldedRemoval {
                        since_ms: *since_ms,
                        named: BTreeSet::new(),
                        pinned: true,
                    },
                };
                Ok((id, removal))
            })
            .collect::<Result<_>>()?,
        overflow: kept
            .overflow
            .as_ref()
            .map(|overflow| {
                Ok::<_, Error>(Overflow {
                    since_ms: overflow.since_ms,
                    named: overflow
                        .named
                        .iter()
                        .map(|(hex, since)| Ok((member(hex)?, *since)))
                        .collect::<Result<_>>()?,
                    pinned: overflow.pinned,
                })
            })
            .transpose()?,
    };
    // Earlier shapes: an overflow by time, or a folded target with its
    // latest arrival and names.
    let pinned = match kept.pinned {
        Some(PinnedKept::Since(since)) => Some(since),
        _ => None,
    };
    for since in [kept.overflowed, kept.since_ms].into_iter().flatten() {
        let overflow = target.overflow.get_or_insert_with(Overflow::default);
        overflow.since_ms = overflow.since_ms.max(since);
        overflow.pinned = overflow.pinned.max(Some(since));
    }
    if let Some(overflow) = &mut target.overflow {
        overflow.pinned = overflow.pinned.max(pinned);
        for (hex, since) in &kept.named {
            let at = overflow.named.entry(member(hex)?).or_insert(*since);
            *at = (*at).max(*since);
        }
    }
    for hex in &kept.named_with {
        member(hex)?;
    }
    target.cap();
    Ok(target)
}

fn unconfirmed(kept: &BTreeMap<String, Kept>) -> Result<BTreeMap<OpId, super::Unconfirmed>> {
    kept.iter()
        .map(|(id, kept)| {
            let id = OpId::from_hex(id).ok_or(Error::Peers)?;
            Ok((
                id,
                super::Unconfirmed {
                    signer: member(&kept.signer)?,
                    targets: kept
                        .targets
                        .iter()
                        .map(|hex| member(hex))
                        .collect::<Result<_>>()?,
                    since_ms: kept.since_ms,
                    // One that no longer reads keeps its suspension.
                    op: kept
                        .op
                        .as_deref()
                        .and_then(|op| sync::bytes(op).ok())
                        .and_then(|op| Op::decode(&op).ok())
                        .filter(|op| op.id() == id),
                },
            ))
        })
        .collect()
}

/// Writes the log and the peers' state. Blocking.
///
/// Each file is replaced whole, and the two in an order that is safe at
/// every step, even if one write fails or the daemon stops between them:
///
/// 1. `peers.json`, keeping every suspension `on_disk` still holds beside
///    the current ones: a suspension the new log settles must outlive the
///    old log on disk.
/// 2. The log, last of what a caller may roll back: a save that fails
///    before it leaves the log on disk as it was.
/// 3. `peers.json` again without the settled suspensions, once the log
///    that settles them is written. A failure here only leaves extra
///    suspensions, which the next load or save settles again.
///
/// `on_disk` is the suspensions `peers.json` holds; kept up to date.
pub(super) fn save(state_dir: &Path, inner: &Inner, on_disk: &mut Suspensions) -> Result<()> {
    let now = Suspensions {
        unconfirmed: inner.unconfirmed.clone(),
        folded: inner.folded.clone(),
    };
    let first = merged(&now, on_disk);
    write_peers(state_dir, inner, &first)?;
    *on_disk = first;
    if let Some(log) = &inner.log {
        write_private(&path(state_dir, LOG), &saved::encode(log)?)?;
        remove_joining(state_dir)?;
    } else {
        save_joining(state_dir, &inner.joining)?;
    }
    if *on_disk != now {
        match write_peers(state_dir, inner, &now) {
            Ok(()) => *on_disk = now,
            Err(error) => {
                tracing::warn!(error = %error, "could not drop settled suspensions; kept until the next save");
            }
        }
    }
    Ok(())
}

/// The suspensions held `now` with those `on_disk` beside them, as the
/// first write of a save keeps them: none dropped, and each folded target
/// still within its bound of ids, the rest in its overflow.
pub(super) fn merged(now: &Suspensions, on_disk: &Suspensions) -> Suspensions {
    let mut first = now.clone();
    for (id, kept) in &on_disk.unconfirmed {
        first.unconfirmed.entry(*id).or_insert_with(|| kept.clone());
    }
    for (signer, targets) in &on_disk.folded {
        let folded = first.folded.entry(*signer).or_default();
        for (target, kept) in targets {
            let entry = folded.entry(*target).or_insert_with(|| kept.clone());
            for (id, removal) in &kept.ids {
                entry.ids.entry(*id).or_insert_with(|| removal.clone());
            }
            if let Some(kept) = &kept.overflow {
                let overflow = entry.overflow.get_or_insert_with(Default::default);
                overflow.since_ms = overflow.since_ms.max(kept.since_ms);
                for (name, since) in &kept.named {
                    let at = overflow.named.entry(*name).or_insert(*since);
                    *at = (*at).max(*since);
                }
                overflow.pinned = overflow.pinned.max(kept.pinned);
            }
            entry.cap();
        }
    }
    first
}

/// Writes `peers.json` from `inner`, with `unconfirmed` as its
/// suspensions.
fn write_peers(state_dir: &Path, inner: &Inner, suspensions: &Suspensions) -> Result<()> {
    let held = |key: fn(&Holder) -> Option<String>| {
        inner
            .held
            .iter()
            .filter_map(|(peer, ids)| {
                Some((key(peer)?, ids.iter().map(ToString::to_string).collect()))
            })
            .collect()
    };
    let peers = Peers {
        folded: Vec::new(),
        held: held(|peer| match peer {
            Holder::Member(member) => Some(member.to_string()),
            Holder::Host(_) => None,
        }),
        held_hosts: held(|peer| match peer {
            Holder::Host(host) => Some(host.to_string()),
            Holder::Member(_) => None,
        }),
        held_also: inner
            .held_also
            .iter()
            .map(|(host, kept)| {
                let kept = kept
                    .iter()
                    .map(|(id, members)| {
                        (
                            id.to_string(),
                            members.iter().map(ToString::to_string).collect(),
                        )
                    })
                    .collect();
                (host.to_string(), kept)
            })
            .collect(),
        proven: inner
            .proven
            .iter()
            .map(|(member, keys)| {
                (
                    member.to_string(),
                    Proven::Many(keys.iter().map(|key| HostId(*key).to_string()).collect()),
                )
            })
            .collect(),
        unknown: inner
            .unknown
            .iter()
            .map(|(id, seen)| (id.to_string(), *seen))
            .collect(),
        folds: suspensions
            .folded
            .iter()
            .map(|(signer, targets)| {
                let targets = targets
                    .iter()
                    .map(|(target, kept)| {
                        let kept = FoldedKept {
                            ids: kept
                                .ids
                                .iter()
                                .map(|(id, removal)| {
                                    let removal = RemovalKept {
                                        since_ms: removal.since_ms,
                                        named: removal
                                            .named
                                            .iter()
                                            .map(ToString::to_string)
                                            .collect(),
                                        pinned: removal.pinned,
                                    };
                                    (id.to_string(), IdKept::Removal(removal))
                                })
                                .collect(),
                            overflow: kept.overflow.as_ref().map(|overflow| OverflowKept {
                                since_ms: overflow.since_ms,
                                named: overflow
                                    .named
                                    .iter()
                                    .map(|(name, since)| (name.to_string(), *since))
                                    .collect(),
                                pinned: overflow.pinned,
                            }),
                            ..FoldedKept::default()
                        };
                        (target.to_string(), kept)
                    })
                    .collect();
                (signer.to_string(), targets)
            })
            .collect(),
        unconfirmed: suspensions
            .unconfirmed
            .iter()
            .map(|(id, kept)| {
                (
                    id.to_string(),
                    Kept {
                        signer: kept.signer.to_string(),
                        targets: kept.targets.iter().map(ToString::to_string).collect(),
                        since_ms: kept.since_ms,
                        op: kept.op.as_ref().map(|op| sync::text(&op.encode())),
                    },
                )
            })
            .collect(),
        proven_version: PROVEN_VERSION,
    };
    let json = serde_json::to_vec(&peers).map_err(|_| crate::desktop::Error::RecordFormat)?;
    write_private(&path(state_dir, PEERS), &json)?;
    Ok(())
}

/// Removes how this computer joined its fleet as a shared host, the log,
/// and any history of a fleet being joined, so a scrub leaves no fleet,
/// and no role in one, behind. The role goes first: kept, it fails the
/// scrub before anything else goes. Blocking.
pub(super) fn remove_log(state_dir: &Path) -> Result<()> {
    remove(&path(state_dir, HOSTED))?;
    remove(&path(state_dir, LOG))?;
    remove_joining(state_dir)
}

/// Removes the peers' state. Blocking.
pub(super) fn remove_peers(state_dir: &Path) -> Result<()> {
    remove(&path(state_dir, PEERS))
}

/// Each history kept in `joining/`, at most [`MAX_FLEETS`]; one that no
/// longer reads, or names another fleet than its file, is passed over.
/// Blocking.
fn load_joining(state_dir: &Path, now_ms: u64) -> Result<BTreeMap<FleetId, Joining>> {
    let dir = path(state_dir, JOINING);
    let entries = match std::fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(BTreeMap::new()),
        Err(source) => {
            return Err(crate::desktop::Error::File {
                operation: "read",
                path: dir,
                source,
            }
            .into());
        }
    };
    let mut joining = BTreeMap::new();
    for entry in entries.flatten().take(MAX_FLEETS * 4) {
        let file = entry.path();
        let Some(fleet) = file
            .file_name()
            .and_then(|name| name.to_str())
            .and_then(|name| name.strip_suffix(".bin"))
            .and_then(FleetId::from_hex)
        else {
            continue;
        };
        let Some(log) = read_bounded(&file, saved::MAX_SAVED as u64)?
            .and_then(|bytes| saved::decode(&bytes, now_ms).ok())
            .filter(|log| log.fleet() == fleet)
        else {
            continue;
        };
        if joining.len() < MAX_FLEETS {
            joining.insert(
                fleet,
                Joining {
                    log,
                    touched_ms: now_ms,
                },
            );
        }
    }
    Ok(joining)
}

/// Writes each history in `joining`, and removes those no longer kept.
/// Blocking.
fn save_joining(state_dir: &Path, joining: &BTreeMap<FleetId, Joining>) -> Result<()> {
    let dir = path(state_dir, JOINING);
    for (fleet, kept) in joining {
        write_private(
            &dir.join(format!("{fleet}.bin")),
            &saved::encode(&kept.log)?,
        )?;
    }
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return Ok(());
    };
    for entry in entries.flatten() {
        let file = entry.path();
        let kept = file
            .file_name()
            .and_then(|name| name.to_str())
            .and_then(|name| name.strip_suffix(".bin"))
            .and_then(FleetId::from_hex)
            .is_some_and(|fleet| joining.contains_key(&fleet));
        if !kept {
            remove(&file)?;
        }
    }
    Ok(())
}

/// Removes every history kept of fleets being joined. Blocking.
fn remove_joining(state_dir: &Path) -> Result<()> {
    let dir = path(state_dir, JOINING);
    match std::fs::remove_dir_all(&dir) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(source) => Err(crate::desktop::Error::File {
            operation: "remove",
            path: dir,
            source,
        }
        .into()),
    }
}

/// Removes `file`, which may not exist.
fn remove(file: &Path) -> Result<()> {
    match std::fs::remove_file(file) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(source) => Err(crate::desktop::Error::File {
            operation: "remove",
            path: file.to_owned(),
            source,
        }
        .into()),
    }
}
