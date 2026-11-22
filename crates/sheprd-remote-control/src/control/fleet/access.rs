//! The access file a plain host's agent takes (`sheprd_relay_agent::
//! access`), as the log computes it: what Sheprd hands
//! `sheprd-relay-agent fleet apply` over SSH (`docs/relay-plan.md`, Fleet,
//! Spreading a host).
//!
//! The format and the agent's merge live in `sheprd-relay-agent`, built
//! without its runtime, so the agent never depends on this crate and this
//! crate brings no Tokio to the app.

use super::{
    Body, Error, HostId, HostKind, Kind, Log, MAX_TOMBSTONES, MemberId, Op, OpId, Result, State,
};
use sheprd_relay_agent::fleet::Suspension;
use std::collections::{BTreeMap, BTreeSet};

pub use sheprd_relay_agent::access::{Access, Id, MAX_SETTLED, Member};

/// A removal a plain host's agent reported suspending `target` for: its
/// id, and the signer and sequence number the agent read in it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Asked {
    pub removal: OpId,
    pub signer: MemberId,
    pub seq: u64,
    pub target: MemberId,
}

/// Whether removal `op` may be passed to a plain host's agent, which acts
/// on it without checking it: every member it names has been removed.
/// A removal the log voided may name a member another removal took, but
/// it also names one that is still a member, and the agent would suspend
/// that one. One that names only removed members suspends no one the log
/// still lets in.
#[must_use]
pub fn for_agents(state: &State, op: &Op) -> bool {
    let Body::Remove(remove) = op.body() else {
        return false;
    };
    remove
        .targets()
        .all(|target| state.tombstones.contains(target))
}

/// For each of `ops`, the members `removed_by` ([`Log::removed_by`]) says
/// it removes that it does not name, at most
/// [`super::removal::MAX_ALSO`]: what a daemon tells a plain host's agent
/// beside the removals, which the agent cannot work out itself. Those
/// beyond reach it in the access file.
#[must_use]
pub fn also(removed_by: &BTreeMap<OpId, BTreeSet<MemberId>>, ops: &[Op]) -> Vec<(OpId, MemberId)> {
    ops.iter()
        .filter_map(|op| {
            let Body::Remove(remove) = op.body() else {
                return None;
            };
            let named: Vec<&MemberId> = remove.targets().collect();
            let reach = removed_by.get(&op.id())?;
            Some(
                reach
                    .iter()
                    .filter(move |member| !named.contains(member))
                    .map(move |member| (op.id(), *member))
                    .collect::<Vec<_>>(),
            )
        })
        .flatten()
        .take(super::removal::MAX_ALSO)
        .collect()
}

/// Every removal the agent's `status --json` reports suspending for.
pub fn asked(suspended: &BTreeMap<Id, Suspension>) -> Vec<Asked> {
    suspended
        .iter()
        .flat_map(|(target, suspension)| {
            suspension.removals.iter().map(|(removal, cause)| Asked {
                removal: OpId(removal.0),
                signer: MemberId(cause.signer.0),
                seq: cause.seq,
                target: MemberId(target.0),
            })
        })
        .collect()
}

/// Whether `asked` no longer stands in `log`, so the agent may lift what it
/// suspended for it. It does when the log holds the operation (a `Remove`
/// whose target is still a member was voided; anything else is no
/// removal). One the log does not hold stands while it waits for its
/// parents; otherwise it is void when its signer or target is no member,
/// when the log holds a later operation of its signer (the log synced with
/// the signer, and the signer never made it), or once `graced` says it
/// stayed unknown long enough.
fn settles(log: &Log, asked: &Asked, graced: &impl Fn(&OpId) -> bool) -> bool {
    if log.get(&asked.removal).is_some() {
        return true;
    }
    if log
        .pending_removals()
        .iter()
        .any(|(op, _)| op.id() == asked.removal)
    {
        return false;
    }
    let state = log.state();
    let Some(signer) = state.members.get(&asked.signer) else {
        return true;
    };
    !state.members.contains_key(&asked.target) || signer.seq >= asked.seq || graced(&asked.removal)
}

// The agent remembers as many removed members as the log does.
const _: () = assert!(sheprd_relay_agent::access::MAX_REMOVED == MAX_TOMBSTONES);

/// How new `member`'s keys are, as the agent orders key updates: the
/// sequence number of its own `Rotate` that put them in place (0 for the
/// keys it was admitted with), which no other operation moves and which a
/// checkpoint keeps, so a pruned log and a whole one agree; then the
/// SHA-256 of the bundle, the same in every log, between rotations that
/// compete at one number because the member's key signed in two places.
fn key_version(member: &crate::control::fleet::Member) -> (u64, Id) {
    let bundle = member
        .bundle
        .to_bytes()
        .map(|bytes| super::sha256(&[&bytes]))
        .unwrap_or_default();
    (member.rotated, Id(bundle))
}

/// The access file for plain host `host` as `log` has it: every live
/// computer member, allowed or not, with the `rev` of its `Access`
/// register, and every removed member. `suspended` are the removals the
/// host's agent reported suspending for (`asked`); each goes in `settled`
/// once it no longer stands ([`settles`], with `graced` for one the log
/// has not heard of), so the agent lifts a suspension the log voided and
/// no member can keep another suspended by naming removals that do not
/// exist. A held host's file is `paused`: its agent lets no member reach
/// SSH until the fleet picks the host's keys, while removals, denials, and
/// rotations still reach it. Refuses a host that is not a plain host.
pub fn compute(
    log: &Log,
    host: &HostId,
    suspended: impl IntoIterator<Item = Asked>,
    graced: impl Fn(&OpId) -> bool,
) -> Result<Access> {
    let state = log.state();
    let entry = state.hosts.get(host).ok_or(Error::Unknown)?;
    if entry.intro.kind != HostKind::Agent {
        return Err(Error::Invalid);
    }
    let members = state
        .members
        .iter()
        .filter(|(_, member)| member.bundle.kind == Kind::Computer)
        .map(|(id, member)| {
            // A member without an Ed25519 key is still listed: leaving it
            // out would leave the agent its older keys and access.
            let ssh = member
                .bundle
                .ssh
                .as_deref()
                .and_then(sheprd_relay_agent::authorized_keys::ed25519);
            let rev = state
                .access
                .get(&(*host, *id))
                .map_or(0, |register| register.rev);
            let version = key_version(member);
            let member = Member {
                // The register alone: a hold is `paused`, never a deny
                // the agent would keep once the hold lifts.
                allow: state.granted(host, id),
                dial: member.bundle.dial,
                fleet_key: member.bundle.key,
                name: member.bundle.name.clone(),
                rev,
                ssh,
                tunnel: member.bundle.tunnel,
                keys: version.0,
                tiebreak: version.1,
                former: Vec::new(),
            };
            (Id(id.0), member)
        })
        .collect();
    // One removal may suspend several members: each id counts once
    // against the bound, so repeats cannot crowd out another's.
    let settled: BTreeSet<Id> = suspended
        .into_iter()
        .filter(|asked| settles(log, asked, &graced))
        .map(|asked| Id(asked.removal.0))
        .collect();
    let settled = settled.into_iter().take(MAX_SETTLED).collect();
    let access = Access {
        fleet: Id(log.fleet().0),
        members,
        paused: entry.held,
        removed: state.tombstones.iter().map(|id| Id(id.0)).collect(),
        settled,
    };
    access.check()?;
    Ok(access)
}
