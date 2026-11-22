//! A log as a member keeps it on disk: the checkpoint it was pruned to,
//! with that checkpoint's state, then every operation it holds. Reading it
//! checks every operation again, as if it had just arrived, so a file
//! changed on disk computes nothing a signature does not back.

use super::{
    Body, Error, Log, MAX_CHECKPOINTS_PER_SIGNER, MAX_COMPUTERS, MAX_MEMBERSHIP_OPS, MAX_OPS,
    MAX_PENDING, MAX_PHONES, MAX_REMOVALS_PER_SIGNER, MAX_STATE, Op, OpId, Result,
};
use std::collections::HashMap;

const MAGIC: &[u8] = b"sheprd-fleet-log-v1";
/// Members whose operations a log can hold between checkpoints: the live
/// members the checkpoint carries, and every one admitted since, each by
/// one of the membership operations the log bounds.
const SIGNERS: usize = MAX_COMPUTERS + MAX_PHONES + MAX_MEMBERSHIP_OPS;
/// Every operation a log can hold: the ordinary ones, and each signer's
/// own share of removals and checkpoints, which count against no other
/// bound (`Log::over_caps`). A saved log never holds more, so reading one
/// back never refuses what the log took.
pub const MAX_HELD: usize =
    MAX_OPS + SIGNERS * (MAX_REMOVALS_PER_SIGNER + MAX_CHECKPOINTS_PER_SIGNER);
/// The largest file read: a full log, a full state, and their lengths.
pub const MAX_SAVED: usize = MAX_STATE + MAX_HELD * (super::MAX_OP + 4) + 1024;

/// The bytes that keep `log`.
pub fn encode(log: &Log) -> Result<Vec<u8>> {
    let mut bytes = MAGIC.to_vec();
    match log.base() {
        None => bytes.push(0),
        Some((root, _)) => {
            bytes.push(1);
            push(&mut bytes, &root.encode())?;
            // The state the checkpoint names, before its own operation
            // counted: what reading it back checks the hash against.
            push(&mut bytes, &log.state_at(&root.id())?.encode()?)?;
        }
    }
    let ops: Vec<Vec<u8>> = log.ops().map(Op::encode).collect();
    bytes.extend_from_slice(
        &u32::try_from(ops.len())
            .map_err(|_| Error::Bounds)?
            .to_be_bytes(),
    );
    for op in &ops {
        push(&mut bytes, op)?;
    }
    Ok(bytes)
}

/// The log `bytes` keep, read at `now_ms`. Operations that no longer
/// check out are dropped; a file without the operation that founded the
/// fleet, or the checkpoint it was pruned to, is refused.
pub fn decode(bytes: &[u8], now_ms: u64) -> Result<Log> {
    if bytes.len() > MAX_SAVED {
        return Err(Error::Bounds);
    }
    let mut rest = bytes.strip_prefix(MAGIC).ok_or(Error::Encoding)?;
    let (&flag, after) = rest.split_first().ok_or(Error::Encoding)?;
    rest = after;
    let root = match flag {
        0 => None,
        1 => {
            let root = Op::decode(take(&mut rest, super::MAX_OP)?)?;
            let state = take(&mut rest, MAX_STATE)?;
            Some((root, state.to_vec()))
        }
        _ => return Err(Error::Encoding),
    };
    let count =
        usize::try_from(u32::from_be_bytes(array(&mut rest)?)).map_err(|_| Error::Bounds)?;
    if count > MAX_HELD {
        return Err(Error::Bounds);
    }
    let mut ops = (0..count)
        .map(|_| Op::decode(take(&mut rest, super::MAX_OP)?))
        .collect::<Result<Vec<_>>>()?;
    if !rest.is_empty() {
        return Err(Error::Encoding);
    }
    let mut log = match root {
        Some((root, state)) => Log::from_checkpoint(root, &state)?,
        None => {
            let found = ops
                .iter()
                .position(|op| matches!(op.body(), Body::Found(_)))
                .ok_or(Error::Invalid)?;
            Log::found(ops.swap_remove(found))?
        }
    };
    // Read as of the latest date the file holds, or now if later: every
    // operation in it passed the skew check when it arrived, and a clock
    // set back since must not drop any, least of all this device's own,
    // whose sequence numbers its next operation would then reuse.
    let at = ops
        .iter()
        .map(|op| op.unsigned().issued_ms)
        .fold(now_ms, u64::max);
    // Parents first, a pool's worth at a time: each batch applies whole,
    // so the bounded pool of operations waiting for their parents never
    // has to drop history the file holds. What no longer checks out is
    // dropped with its reason; the rest stands.
    let mut ops = parents_first(ops).into_iter().peekable();
    while ops.peek().is_some() {
        let batch: Vec<Op> = ops.by_ref().take(MAX_PENDING).collect();
        let _ = log.insert_all(batch, at);
    }
    Ok(log)
}

/// The operations `bytes` hold, read for their shape alone, as far as
/// the file can be read: no signature is checked, so they are only ever
/// handed to a log that checks them. What a device keeps of a copy that no
/// longer checks out, so the operations it signed and no peer holds yet
/// are not lost when it starts its copy again.
#[must_use]
pub fn salvage(bytes: &[u8]) -> Vec<Op> {
    let mut ops = Vec::new();
    if bytes.len() > MAX_SAVED {
        return ops;
    }
    let Some(mut rest) = bytes.strip_prefix(MAGIC) else {
        return ops;
    };
    let Some((&flag, after)) = rest.split_first() else {
        return ops;
    };
    rest = after;
    if flag == 1 {
        let root = take(&mut rest, super::MAX_OP).and_then(Op::decode);
        if take(&mut rest, MAX_STATE).is_err() {
            return ops;
        }
        ops.extend(root);
    }
    let Ok(count) = array(&mut rest).map(u32::from_be_bytes) else {
        return ops;
    };
    for _ in 0..count.min(u32::try_from(MAX_HELD).unwrap_or(u32::MAX)) {
        let Ok(op) = take(&mut rest, super::MAX_OP) else {
            break;
        };
        ops.extend(Op::decode(op));
    }
    ops
}

/// `ops` ordered so each comes after every parent among them; those whose
/// parents the file lacks come last, in no meaningful order.
fn parents_first(ops: Vec<Op>) -> Vec<Op> {
    let index: HashMap<OpId, usize> = ops.iter().enumerate().map(|(i, op)| (op.id(), i)).collect();
    let mut waiting = vec![0_usize; ops.len()];
    let mut children = vec![Vec::new(); ops.len()];
    for (position, op) in ops.iter().enumerate() {
        for parent in op.parents() {
            if let Some(&parent) = index.get(parent) {
                waiting[position] += 1;
                children[parent].push(position);
            }
        }
    }
    let mut ready: Vec<usize> = (0..ops.len()).filter(|&i| waiting[i] == 0).collect();
    let mut order = Vec::with_capacity(ops.len());
    while let Some(position) = ready.pop() {
        order.push(position);
        for &child in &children[position] {
            waiting[child] -= 1;
            if waiting[child] == 0 {
                ready.push(child);
            }
        }
    }
    let mut slots: Vec<Option<Op>> = ops.into_iter().map(Some).collect();
    let mut sorted: Vec<Op> = order.iter().filter_map(|&i| slots[i].take()).collect();
    // Ids are hashes, so no cycle can hold an operation back; whatever
    // could is kept rather than lost.
    sorted.extend(slots.into_iter().flatten());
    sorted
}

fn push(bytes: &mut Vec<u8>, item: &[u8]) -> Result<()> {
    bytes.extend_from_slice(
        &u32::try_from(item.len())
            .map_err(|_| Error::Bounds)?
            .to_be_bytes(),
    );
    bytes.extend_from_slice(item);
    Ok(())
}

fn array(rest: &mut &[u8]) -> Result<[u8; 4]> {
    let (head, tail) = rest.split_at_checked(4).ok_or(Error::Encoding)?;
    *rest = tail;
    head.try_into().map_err(|_| Error::Encoding)
}

fn take<'a>(rest: &mut &'a [u8], max: usize) -> Result<&'a [u8]> {
    let length = usize::try_from(u32::from_be_bytes(array(rest)?)).map_err(|_| Error::Bounds)?;
    if length > max {
        return Err(Error::Bounds);
    }
    let (head, tail) = rest.split_at_checked(length).ok_or(Error::Encoding)?;
    *rest = tail;
    Ok(head)
}
