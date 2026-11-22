//! This computer's favorite agents on disk (`control::favorites`), and
//! `sheprd-remote-control favorites`, which Sheprd runs over SSH on another
//! computer. A host may have the program but no daemon running, so the file
//! stands in for one.

mod error;

pub use error::Error;

use crate::control::{
    self, Request, Response,
    favorites::{Change, Favorites},
};
use std::{
    fs,
    path::{Path, PathBuf},
};

const FILE: &str = "favorites.json";

/// Where the favorites of the daemon whose state is `state_dir` are kept.
pub fn file(state_dir: &Path) -> PathBuf {
    state_dir.join(FILE)
}

/// The favorites saved in `file`, bounded; none when it is missing or
/// unreadable, as a damaged file costs only the stars. Blocking.
pub fn load(file: &Path) -> Favorites {
    fs::read(file)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Favorites>(&bytes).ok())
        .map(Favorites::bounded)
        .unwrap_or_default()
}

/// `change` made to `saved`, refused when it names no valid pane.
pub fn changed(saved: Favorites, change: Change) -> super::Result<Favorites> {
    let pane = change.pane.clone();
    saved
        .changed(change)
        .ok_or_else(|| Error::Pane { pane }.into())
}

/// Makes `change`, if any, to the favorites in `state_dir` and returns
/// those that stand: through the daemon when one runs, so its phones hear
/// of it at once, else in the file it reads when it starts. Blocking.
pub fn exchange(state_dir: &Path, change: Option<Change>) -> super::Result<Favorites> {
    let address = control::address(state_dir);
    let request = Request::Favorites {
        change: change.clone(),
    };
    match control::request(&address, &request) {
        Ok(Response::Favorites(favorites)) => Ok(favorites),
        Ok(_) => Err(super::Error::Unexpected),
        Err(control::Error::Connect(_)) => change_file(state_dir, change),
        Err(error) => Err(error.into()),
    }
}

/// With no daemon to ask: makes `change`, if any, in `state_dir`'s file.
/// Blocking.
fn change_file(state_dir: &Path, change: Option<Change>) -> super::Result<Favorites> {
    let file = file(state_dir);
    let Some(change) = change else {
        return Ok(load(&file));
    };
    fs::create_dir_all(state_dir).map_err(|source| super::Error::Io {
        operation: "create",
        path: state_dir.to_owned(),
        source,
    })?;
    super::edited::locked(&file, || {
        let saved = load(&file);
        let next = changed(saved.clone(), change)?;
        if next != saved {
            super::edited::save(&file, &next)?;
        }
        Ok(next)
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests;
