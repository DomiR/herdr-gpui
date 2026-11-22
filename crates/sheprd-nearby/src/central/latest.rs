//! A mailbox of one: the latest value asked for, for a worker that applies
//! changes one at a time, in order. A change asked for while the worker is
//! busy replaces any still waiting, so the worker always ends on the last
//! one asked and never holds more than one.

use std::sync::{Condvar, Mutex, MutexGuard, PoisonError};

#[derive(Debug)]
pub(crate) struct Latest<T> {
    slot: Mutex<Slot<T>>,
    ready: Condvar,
}

#[derive(Debug)]
struct Slot<T> {
    value: Option<T>,
    closed: bool,
}

impl<T> Default for Latest<T> {
    fn default() -> Self {
        Self {
            slot: Mutex::new(Slot {
                value: None,
                closed: false,
            }),
            ready: Condvar::new(),
        }
    }
}

impl<T> Latest<T> {
    /// Asks for `value`, replacing what the worker has not taken yet.
    pub(crate) fn put(&self, value: T) {
        self.lock().value = Some(value);
        self.ready.notify_one();
    }

    /// Ends the worker once it has taken what is waiting.
    pub(crate) fn close(&self) {
        self.lock().closed = true;
        self.ready.notify_one();
    }

    /// Waits for the next value; `None` once closed and empty.
    pub(crate) fn take(&self) -> Option<T> {
        let mut slot = self.lock();
        loop {
            if let Some(value) = slot.value.take() {
                return Some(value);
            }
            if slot.closed {
                return None;
            }
            slot = self
                .ready
                .wait(slot)
                .unwrap_or_else(PoisonError::into_inner);
        }
    }

    fn lock(&self) -> MutexGuard<'_, Slot<T>> {
        self.slot.lock().unwrap_or_else(PoisonError::into_inner)
    }
}
