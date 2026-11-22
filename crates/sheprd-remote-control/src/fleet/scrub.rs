//! `fleet scrub`: forgets the fleet here, without signing anything, with
//! every way it let someone in: the pairings of desktops whose keys
//! members proved, the phones its links admitted to the API, the members'
//! `authorized_keys` lines, and the relay records it wrote. The fleet key
//! stays.
//!
//! It fails closed: it answers only once access is gone, and what it could
//! not read (a damaged peers file, whose proven keys it could not revoke)
//! stays on disk and is reported, so asking again finishes the job. A
//! damaged log or record list never stops the rest.

use super::{Fleet, Result, grants, links, store};
use crate::ssh_access::{self, AuthorizedKeys};
use crate::web::State;
use std::path::Path;

/// The daemon's answer to `Ask::Scrub`. Blocking.
pub(crate) fn live(state: &State, fleet: &Fleet, session: Option<u16>) -> Result<()> {
    // A grants write already running finishes first, and none starts
    // until the fleet is forgotten.
    let _writing = fleet.writing();
    // As if every member were removed: pairings with keys they proved here
    // go first.
    for tunnel in fleet.proven_keys() {
        grants::supersede(state, &tunnel)?;
    }
    links::forget(&mut state.auth(), fleet.state_dir())?;
    // Access goes even when a file stays; the failure is still reported.
    // Located before anything goes: a scrub that cannot find the lines
    // it must remove says so rather than leave them.
    let keys = daemon_keys(state)?;
    let forgotten = fleet.scrub(session);
    forget_access(keys.as_ref(), fleet.state_dir())?;
    forgotten
}

/// The daemon's answer to `Ask::Scrub` when its fleet did not load: a
/// damaged file, which a scrub must still clear. Blocking.
pub(crate) fn unloaded(state: &State) -> Result<()> {
    let state_dir = state.auth().state_dir().to_owned();
    let keys = daemon_keys(state)?;
    on_disk(&state_dir, keys.as_ref(), |tunnels| {
        for tunnel in tunnels {
            grants::supersede(state, tunnel)?;
        }
        Ok(links::forget(&mut state.auth(), &state_dir)?)
    })
}

/// `fleet scrub` with no daemon running. Blocking.
pub(crate) fn offline(state_dir: &Path) -> Result<()> {
    let keys = located(None, AuthorizedKeys::of_user)?;
    on_disk(state_dir, keys.as_ref(), |tunnels| {
        // Read whether or not members proved keys: phones the fleet's
        // links admitted go too.
        let mut auth = crate::auth::Auth::load(state_dir, crate::auth::Limits::default())?;
        for tunnel in tunnels {
            grants::revoke_paired(&mut auth, keys.as_ref(), tunnel)?;
        }
        Ok(links::forget(&mut auth, state_dir)?)
    })
}

/// The `authorized_keys` the daemon lets members in through, or the
/// user's own when it started without them: lines an earlier run wrote
/// may still be there.
fn daemon_keys(state: &State) -> Result<Option<AuthorizedKeys>> {
    let running = state
        .auth()
        .ssh_access()
        .ok()
        .map(|access| access.keys.clone());
    located(running, AuthorizedKeys::of_user)
}

/// `known`, or the file `locate` finds. Only where the fleet never writes
/// lines (`Unsupported`) are there none to remove; any other failure to
/// find them, such as no usable `HOME`, fails the scrub, which must not
/// say access is gone while lines may remain.
pub(super) fn located(
    known: Option<AuthorizedKeys>,
    locate: impl FnOnce() -> ssh_access::Result<AuthorizedKeys>,
) -> Result<Option<AuthorizedKeys>> {
    if known.is_some() {
        return Ok(known);
    }
    match locate() {
        Ok(keys) => Ok(Some(keys)),
        Err(ssh_access::Error::Unsupported) => Ok(None),
        Err(error) => Err(error.into()),
    }
}

/// Removes the members' lines from `keys` and the fleet's relay records.
fn forget_access(keys: Option<&AuthorizedKeys>, state_dir: &Path) -> Result<()> {
    if let Some(keys) = keys {
        keys.set_fleet(&[])?;
    }
    grants::forget_records(state_dir)
}

/// Forgets the fleet kept in `state_dir` without reading its log, with
/// `revoke` taking the pairings of the keys members proved. Blocking.
fn on_disk(
    state_dir: &Path,
    keys: Option<&AuthorizedKeys>,
    revoke: impl FnOnce(&[[u8; 32]]) -> Result<()>,
) -> Result<()> {
    // Read before anything goes; a peers file that does not read is kept,
    // and reported once the rest is gone.
    let proven = store::proven_keys(state_dir);
    let removed = store::remove_log(state_dir);
    forget_access(keys, state_dir)?;
    removed?;
    revoke(&proven?)?;
    store::remove_peers(state_dir)
}
