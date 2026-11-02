use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

#[derive(Clone)]
pub(super) struct Pool {
    open: Arc<AtomicUsize>,
    limit: usize,
}

impl Pool {
    pub(super) fn new(limit: usize) -> Self {
        Self {
            open: Arc::new(AtomicUsize::new(0)),
            limit,
        }
    }

    pub(super) fn try_acquire(&self) -> Option<Permit> {
        let open = self.open.fetch_add(1, Ordering::AcqRel);
        if open >= self.limit {
            self.open.fetch_sub(1, Ordering::AcqRel);
            return None;
        }
        Some(Permit {
            open: Arc::clone(&self.open),
        })
    }

    #[cfg(test)]
    pub(super) fn in_use(&self) -> usize {
        self.open.load(Ordering::Acquire)
    }
}

pub(super) struct Permit {
    open: Arc<AtomicUsize>,
}

impl Drop for Permit {
    fn drop(&mut self) {
        self.open.fetch_sub(1, Ordering::AcqRel);
    }
}
