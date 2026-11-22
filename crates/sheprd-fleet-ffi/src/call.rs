//! The calls the app makes, each on the log bytes it keeps.

use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sheprd_remote_control::control::fleet::{
    self, Body, Inserted, Link, Log, MemberId, Op, OpId, Reason, Remove, View, phone::Linked,
    saved, sync,
};
use std::panic::{AssertUnwindSafe, catch_unwind};

/// Operations one sync may carry: a member daemon refuses more.
pub const MAX_SYNC_OPS: usize = 64;
/// Operations one call takes: a sync answer's worth many times over.
const MAX_CALL_OPS: usize = 4096;
/// Heads one sync may name: a member daemon refuses more.
const MAX_SYNC_HEADS: usize = 64;
/// Operations one draft signs, under one Face ID: a link to every host
/// the fleet can hold, as adding them all at once asks.
pub const MAX_BODIES: usize = fleet::MAX_HOSTS;
/// Heads one call names.
const MAX_HEADS: usize = 1024;

/// One request. Ids are hex, operations and logs base64url, times
/// milliseconds since the epoch.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "snake_case", tag = "call")]
pub enum Call {
    /// Takes `ops` into the log: what a computer answered, or what this
    /// phone signed before. With no `log` yet, `ops` must hold the
    /// operation that founded `fleet`, the fleet this phone joined: that
    /// is the only one it starts from, so no computer can hand it another
    /// fleet's history.
    Apply {
        fleet: String,
        me: String,
        #[serde(default)]
        log: Option<String>,
        #[serde(default)]
        ops: Vec<String>,
        now_ms: u64,
    },
    /// What a computer whose heads are `heads` lacks, at most one sync's
    /// worth, in graph order.
    Missing {
        fleet: String,
        log: String,
        heads: Vec<String>,
        now_ms: u64,
    },
    /// Drafts one operation per body for `me`, each on the one before, on
    /// the log's own heads and sequence, and refuses the lot unless each
    /// would apply once signed (`Log::drafts`). Nothing is kept: the app
    /// signs the bytes and seals them.
    Draft {
        fleet: String,
        me: String,
        log: String,
        issued_ms: u64,
        now_ms: u64,
        bodies: Vec<Draft>,
    },
    /// The operations `me` signed that a saved log holds, read for their
    /// shape alone from a log that no longer checks out, so a copy started
    /// again takes them back, checking each.
    Salvage { me: String, log: String },
    /// Adds operations `me` just signed. Each must apply; otherwise none is
    /// kept and the log answered is the one given.
    Seal {
        fleet: String,
        me: String,
        log: String,
        ops: Vec<String>,
        now_ms: u64,
    },
}

/// What a phone may ask to sign.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "snake_case", tag = "body")]
pub enum Draft {
    Remove {
        member: String,
        #[serde(default)]
        also_members: Vec<String>,
        #[serde(default)]
        also_hosts: Vec<String>,
        reason: Reason,
    },
    HostRemove {
        host: String,
    },
    Link {
        host: String,
        /// P-256, SEC1, base64url.
        device_key: String,
    },
}

/// The log after `apply` or `seal`, and what it tells this phone.
#[derive(Debug, Serialize)]
struct Replica {
    log: String,
    /// What to name as heads when asking a computer for what it lacks.
    known: Vec<String>,
    applied: usize,
    pending: usize,
    refused: usize,
    status: Status,
}

/// What the log tells this phone, in the shape of `GET /api/v2/fleet`, so
/// the app reads both the same way.
#[derive(Debug, Serialize)]
struct Status {
    view: View,
    heads: Vec<String>,
    /// `member` while this phone is live in the log.
    #[serde(skip_serializing_if = "Option::is_none")]
    join: Option<&'static str>,
    #[serde(flatten)]
    linked: Linked,
    /// This phone was removed: it may sign nothing more.
    removed: bool,
}

#[derive(Debug, Serialize)]
struct Drafted {
    /// The bytes to sign, base64url.
    signed: String,
    id: String,
    seq: u64,
    parents: Vec<String>,
}

/// Answers one request: `{"ok": …}`, or `{"error": {"kind", "rule"?,
/// "message"}}`. A panic is answered as an error, never unwound into the
/// app.
#[must_use]
pub fn respond(request: &[u8]) -> Vec<u8> {
    let answer = catch_unwind(AssertUnwindSafe(|| {
        serde_json::from_slice::<Call>(request)
            .map_err(Error::from)
            .and_then(handle)
    }))
    .unwrap_or(Err(Error::Panicked));
    match answer {
        Ok(value) => serde_json::to_vec(&json!({ "ok": value }))
            .unwrap_or_else(|_| refusal(&Error::Panicked)),
        Err(error) => refusal(&error),
    }
}

pub(crate) fn refusal(error: &Error) -> Vec<u8> {
    let body = json!({ "error": {
        "kind": error.kind(),
        "rule": error.rule(),
        "message": error.to_string(),
    }});
    serde_json::to_vec(&body).unwrap_or_else(|_| br#"{"error":{"kind":"panicked"}}"#.to_vec())
}

fn handle(call: Call) -> Result<Value> {
    let value = match call {
        Call::Apply {
            fleet,
            me,
            log,
            ops,
            now_ms,
        } => {
            let me = sync::member(&me)?;
            let mut ops = decode(&ops, MAX_CALL_OPS)?;
            let mut log = match log {
                Some(log) => open(&fleet, &log, now_ms)?,
                None => found(&fleet, &mut ops)?,
            };
            let counts = Counts::of(&log.insert_all(ops, now_ms));
            serde_json::to_value(replica(&log, &me, counts, now_ms)?)?
        }
        Call::Missing {
            fleet,
            log,
            heads,
            now_ms,
        } => {
            let log = open(&fleet, &log, now_ms)?;
            if heads.len() > MAX_HEADS {
                return Err(Error::Bounds);
            }
            let heads = sync::Apply {
                heads,
                ..sync::Apply::default()
            }
            .heads()?;
            missing(&log, &heads)
        }
        Call::Draft {
            fleet,
            me,
            log,
            issued_ms,
            now_ms,
            bodies,
        } => {
            if bodies.is_empty() || bodies.len() > MAX_BODIES {
                return Err(Error::Bounds);
            }
            let me = sync::member(&me)?;
            let log = open(&fleet, &log, now_ms)?;
            let bodies = bodies.into_iter().map(body).collect::<Result<_>>()?;
            let drafts = log
                .drafts(me, issued_ms, bodies, now_ms)?
                .into_iter()
                .map(|unsigned| {
                    let signed = unsigned.encode()?;
                    let id = Op::decode(&[signed.as_slice(), &[0; 64]].concat())?.id();
                    Ok(Drafted {
                        signed: sync::text(&signed),
                        id: id.to_string(),
                        seq: unsigned.seq,
                        parents: unsigned.parents.iter().map(ToString::to_string).collect(),
                    })
                })
                .collect::<Result<Vec<_>>>()?;
            json!({ "drafts": drafts })
        }
        Call::Salvage { me, log } => {
            let me = sync::member(&me)?;
            let bytes = sync::bytes(&log)?;
            let ops = saved::salvage(&bytes);
            let mine = ops.iter().filter(|op| op.signer() == me);
            json!({ "ops": sync::encode_ops(mine) })
        }
        Call::Seal {
            fleet,
            me,
            log,
            ops,
            now_ms,
        } => {
            let me = sync::member(&me)?;
            let ops = decode(&ops, MAX_BODIES)?;
            if ops.is_empty() {
                return Err(Error::Bounds);
            }
            if ops.iter().any(|op| op.signer() != me) {
                return Err(Error::NotMine);
            }
            let mut log = open(&fleet, &log, now_ms)?;
            let results = log.insert_all(ops, now_ms);
            for (_, result) in &results {
                match result {
                    Ok(Inserted::Applied | Inserted::Known) => {}
                    Ok(Inserted::Pending) => return Err(fleet::Error::ParentMissing.into()),
                    Err(error) => return Err(error.clone().into()),
                }
            }
            serde_json::to_value(replica(&log, &me, Counts::of(&results), now_ms)?)?
        }
    };
    Ok(value)
}

/// The saved log `text`, checked again, which must be `fleet`'s.
fn open(fleet: &str, text: &str, now_ms: u64) -> Result<Log> {
    let fleet = sync::fleet(fleet)?;
    let bytes = sync::bytes(text)?;
    if bytes.len() > saved::MAX_SAVED {
        return Err(Error::Bounds);
    }
    let log = saved::decode(&bytes, now_ms)?;
    if log.fleet() != fleet {
        return Err(Error::OtherFleet);
    }
    // Reading back drops what no longer checks out, which a daemon lives
    // with. The phone's copy must hold all of it: an operation of its own
    // dropped here would be lost, and its sequence number signed again.
    let held = log.ops().count() + usize::from(log.root().is_some());
    if held != saved::salvage(&bytes).len() {
        return Err(Error::Damaged);
    }
    Ok(log)
}

/// What the phone names as its heads when it asks a computer for what it
/// lacks: its heads, then its latest operations, newest first, up to what
/// one sync may name. A computer passes over the ids it does not hold, so
/// when the phone's heads are its own operations the computer has not
/// seen yet, the history they share still counts, and the answer does not
/// start again from the founding operation.
fn known(log: &Log) -> Vec<String> {
    let heads = log.heads().iter().copied();
    let latest = log.missing_for(&[]).into_iter().rev().map(Op::id);
    let mut known: Vec<OpId> = Vec::with_capacity(MAX_SYNC_HEADS);
    for id in heads.chain(latest) {
        if known.len() == MAX_SYNC_HEADS {
            break;
        }
        if !known.contains(&id) {
            known.push(id);
        }
    }
    known.iter().map(ToString::to_string).collect()
}

/// A new log, from the operation in `ops` that founded `fleet`.
fn found(fleet: &str, ops: &mut Vec<Op>) -> Result<Log> {
    let fleet = sync::fleet(fleet)?;
    let position = ops
        .iter()
        .position(|op| matches!(op.body(), Body::Found(_)) && op.id().0 == fleet.0)
        .ok_or(Error::Unfounded)?;
    Ok(Log::found(ops.swap_remove(position))?)
}

fn decode(ops: &[String], max: usize) -> Result<Vec<Op>> {
    if ops.len() > max {
        return Err(Error::Bounds);
    }
    Ok(sync::decode_ops(ops)?)
}

/// What a peer whose heads are `heads` lacks: at most what one sync
/// carries, and `more` when there is more.
fn missing(log: &Log, heads: &[OpId]) -> Value {
    let mut ops = Vec::new();
    let mut bytes = 0;
    let mut more = false;
    for op in log.missing_for(heads) {
        let encoded = op.encode();
        if ops.len() == MAX_SYNC_OPS || bytes + encoded.len() > sync::MAX_ANSWER_BYTES {
            more = true;
            break;
        }
        bytes += encoded.len();
        ops.push(sync::text(&encoded));
    }
    json!({ "ops": ops, "more": more })
}

fn body(draft: Draft) -> Result<Body> {
    let members = |ids: &[String]| {
        ids.iter()
            .map(|id| sync::member(id))
            .collect::<Result<_, _>>()
    };
    let hosts = |ids: &[String]| {
        ids.iter()
            .map(|id| sync::host(id))
            .collect::<Result<_, _>>()
    };
    Ok(match draft {
        Draft::Remove {
            member,
            also_members,
            also_hosts,
            reason,
        } => Body::Remove(Remove {
            member: sync::member(&member)?,
            also_members: members(&also_members)?,
            also_hosts: hosts(&also_hosts)?,
            reason,
        }),
        Draft::HostRemove { host } => Body::HostRemove(sync::host(&host)?),
        Draft::Link { host, device_key } => Body::Link(Link {
            host: sync::host(&host)?,
            device_key: sync::bytes(&device_key)?,
        }),
    })
}

/// How many of a call's operations applied, wait for parents, or were
/// refused; one already held counts as none of them.
#[derive(Clone, Copy, Debug, Default)]
struct Counts {
    applied: usize,
    pending: usize,
    refused: usize,
}

impl Counts {
    fn of(results: &[(OpId, fleet::Result<Inserted>)]) -> Self {
        results
            .iter()
            .fold(Self::default(), |mut counts, (_, result)| {
                match result {
                    Ok(Inserted::Applied) => counts.applied += 1,
                    Ok(Inserted::Pending) => counts.pending += 1,
                    Ok(Inserted::Known) => {}
                    Err(_) => counts.refused += 1,
                }
                counts
            })
    }
}

fn replica(log: &Log, me: &MemberId, counts: Counts, now_ms: u64) -> Result<Replica> {
    let live = log.is_member(me);
    Ok(Replica {
        log: sync::text(&saved::encode(log)?),
        known: known(log),
        applied: counts.applied,
        pending: counts.pending,
        refused: counts.refused,
        status: Status {
            view: View::of(log, me, &[]),
            heads: log.heads().iter().map(ToString::to_string).collect(),
            join: live.then_some("member"),
            linked: Linked::of(log, Some(me), now_ms),
            removed: log.state().tombstones.contains(me),
        },
    })
}
