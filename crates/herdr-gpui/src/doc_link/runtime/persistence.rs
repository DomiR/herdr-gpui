//! Coalesced, ordered saves. The worker never holds its lock during disk I/O.

use super::*;
use std::sync::Mutex;

#[derive(Default)]
struct Mailbox {
    error: Option<Arc<Error>>,
    pending: Option<Vec<Record>>,
    running: bool,
}

pub(super) struct Persistence {
    mailbox: Arc<Mutex<Mailbox>>,
    path: Option<PathBuf>,
    task: Option<Task<()>>,
}

impl Persistence {
    pub(super) fn new(path: Option<PathBuf>) -> Self {
        Self {
            mailbox: Arc::default(),
            path,
            task: None,
        }
    }

    fn save(&mut self, records: Vec<Record>, cx: &App) {
        let Some(path) = self.path.clone() else {
            return;
        };
        let Ok(mut state) = self.mailbox.lock() else {
            return;
        };
        state.pending = Some(records);
        if state.running {
            return;
        }
        state.running = true;
        drop(state);
        let mailbox = self.mailbox.clone();
        self.task = Some(cx.background_executor().spawn(async move {
            loop {
                let records = {
                    let Ok(mut state) = mailbox.lock() else {
                        return;
                    };
                    match state.pending.take() {
                        Some(records) => records,
                        None => {
                            state.running = false;
                            return;
                        }
                    }
                };
                if let Err(error) = crate::doc_link::save(&path, &records) {
                    tracing::warn!(%error, "Could not save document agent links");
                    if let Ok(mut state) = mailbox.lock() {
                        state.error = Some(Arc::new(error));
                    }
                }
            }
        }));
    }

    fn error(&self) -> Option<Arc<Error>> {
        self.mailbox.lock().ok()?.error.take()
    }
}

impl Links {
    fn records(&self) -> Vec<Record> {
        let mut entries: Vec<_> = self.entries.iter().collect();
        entries.sort_by_key(|link| link.used);
        entries
            .into_iter()
            .filter_map(|link| link.record.clone())
            .collect()
    }

    pub(super) fn save(&mut self, cx: &App) {
        self.persistence.save(self.records(), cx);
    }

    pub(in crate::doc_link) fn poll_storage(&mut self, cx: &mut Context<Self>) {
        if let Some(error) = self.persistence.error() {
            for link in &mut self.entries {
                link.error = Some(error.clone());
            }
            cx.notify();
        }
    }

    pub(super) fn shutdown(&mut self, cx: &mut Context<Self>) -> Option<Task<()>> {
        self.quitting = true;
        for link in &mut self.entries {
            link.timer = None;
            if let Some(job) = &link.job {
                job.cancelled.store(true, Ordering::Release);
            }
        }
        self.save(cx);
        self.persistence.task.take()
    }
}
