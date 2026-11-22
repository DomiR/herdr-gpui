//! The fleet folder's lock (`fleet/lock`): the daemon holds it for its
//! whole life, and `fleet apply` with no daemon running holds it while it
//! reads, changes, and writes the log. Two offline updates, or one and a
//! daemon starting, then never each change their own copy of the same log
//! and the last to write drop what the other took.

use super::{Error, Result};
use crate::desktop::files::create_private_dir;
use std::{
    fs::{File, OpenOptions, TryLockError},
    path::Path,
    thread,
    time::{Duration, Instant},
};

/// How long the daemon waits for an offline update to finish, and an
/// offline update for a daemon starting to answer.
pub(crate) const WAIT: Duration = if cfg!(test) {
    Duration::from_secs(2)
} else {
    Duration::from_secs(10)
};
/// How often a busy lock is tried again.
const POLL: Duration = Duration::from_millis(50);

/// Held until dropped; the operating system releases it if the process
/// dies.
#[derive(Debug)]
pub(crate) struct Lock {
    _file: File,
}

impl Lock {
    /// The lock, or `None` while another process holds it. Blocking.
    pub(crate) fn try_acquire(state_dir: &Path) -> Result<Option<Self>> {
        let directory = state_dir.join(super::store::DIRECTORY);
        create_private_dir(&directory)?;
        let path = directory.join("lock");
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(&path)
            .map_err(|source| file_error(&path, source))?;
        match file.try_lock() {
            Ok(()) => Ok(Some(Self { _file: file })),
            Err(TryLockError::WouldBlock) => Ok(None),
            Err(TryLockError::Error(source)) => Err(file_error(&path, source)),
        }
    }

    /// The lock, waiting up to [`WAIT`] for another process to let it go.
    /// Blocking.
    pub(crate) fn acquire(state_dir: &Path) -> Result<Self> {
        let deadline = Instant::now() + WAIT;
        loop {
            if let Some(lock) = Self::try_acquire(state_dir)? {
                return Ok(lock);
            }
            if Instant::now() >= deadline {
                return Err(Error::Busy);
            }
            thread::sleep(POLL);
        }
    }
}

/// Sleeps between tries of a busy lock.
pub(crate) fn pause() {
    thread::sleep(POLL);
}

fn file_error(path: &Path, source: std::io::Error) -> Error {
    Error::Desktop(crate::desktop::Error::File {
        operation: "lock",
        path: path.to_owned(),
        source,
    })
}
