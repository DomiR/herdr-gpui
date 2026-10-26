//! The worker's side of the new issue dialog: reading what it offers, once
//! a session unless that failed or the key changed, and creating the issue
//! it asks for. Both go to Linear from this thread, the key's only holder;
//! the snapshot carries the answers back.

use super::{Command, Service, Worker, linear::LinearKey};
use crate::{Error, linear};
use std::sync::Arc;

/// What the dialog asks the worker.
pub(super) enum Ask {
    Options,
    Create(Box<linear::NewIssue>),
}

/// The dialog's answers, in the snapshot.
#[derive(Clone, Debug, Default)]
pub struct LinearCreate {
    /// The choices once read, or why they could not be.
    pub options: Option<Result<Arc<linear::Options>, Arc<Error>>>,
    /// The newest creation the worker finished.
    pub created: Option<Outcome>,
}

/// How the creation numbered `sequence` ended.
#[derive(Clone, Debug)]
pub struct Outcome {
    pub sequence: u64,
    pub result: Result<linear::Created, Arc<Error>>,
}

impl Service {
    /// Asks for what the new issue dialog offers. `false` means the worker
    /// is behind and nothing was queued.
    pub fn read_linear_options(&self) -> bool {
        self.commands
            .try_send(Command::LinearCreate(Ask::Options))
            .is_ok()
    }

    /// Asks for `issue` to be created. `false` means the worker is behind
    /// and nothing was queued.
    pub fn create_linear_issue(&self, issue: linear::NewIssue) -> bool {
        self.commands
            .try_send(Command::LinearCreate(Ask::Create(Box::new(issue))))
            .is_ok()
    }

    /// A service whose commands wait for the test to read them, never
    /// served.
    #[cfg(any(test, feature = "test-support"))]
    pub fn recording() -> (Self, Recorder) {
        let (commands, receiver) = std::sync::mpsc::sync_channel(super::COMMANDS);
        let service = Self {
            commands,
            mailbox: Arc::default(),
            notices: Arc::default(),
            cancelled: Arc::default(),
        };
        (service, Recorder(receiver))
    }
}

/// What a [`Service::recording`] was asked.
#[cfg(any(test, feature = "test-support"))]
pub struct Recorder(std::sync::mpsc::Receiver<Command>);

#[cfg(any(test, feature = "test-support"))]
impl Recorder {
    /// The new issue dialog's asks since the last call: how many times its
    /// choices were asked for, and the issues to create.
    pub fn linear_asks(&self) -> (usize, Vec<linear::NewIssue>) {
        let mut options = 0;
        let mut issues = Vec::new();
        for command in self.0.try_iter() {
            match command {
                Command::LinearCreate(Ask::Options) => options += 1,
                Command::LinearCreate(Ask::Create(issue)) => issues.push(*issue),
                _ => {}
            }
        }
        (options, issues)
    }
}

impl Worker {
    /// Answers `ask`, then publishes; whether an issue was created, so
    /// the listing syncs to show it.
    pub(super) fn linear_create(&mut self, ask: Ask) -> bool {
        let created = match ask {
            Ask::Options => {
                self.read_linear_options();
                false
            }
            Ask::Create(issue) => self.create_linear_issue(&issue),
        };
        self.publish();
        created
    }

    /// Reads the dialog's choices unless they were read already.
    fn read_linear_options(&mut self) {
        if matches!(self.snapshot.linear_create.options, Some(Ok(_))) {
            return;
        }
        let fetch = self.linear_fetch;
        let read = self.with_linear_key(|key| {
            linear::options(|query, variables| fetch(key, query, variables))
        });
        self.snapshot.linear_create.options = Some(
            read.map(Arc::new)
                .map_err(|source| Arc::new(Error::LinearOptions(source))),
        );
    }

    fn create_linear_issue(&mut self, issue: &linear::NewIssue) -> bool {
        let fetch = self.linear_fetch;
        let result = self
            .with_linear_key(|key| {
                linear::create_issue(|query, variables| fetch(key, query, variables), issue)
            })
            .map_err(|source| Arc::new(Error::LinearCreate(source)));
        let created = result.is_ok();
        self.snapshot.linear_create.created = Some(Outcome {
            sequence: issue.sequence,
            result,
        });
        created
    }

    /// Runs `ask` with the Linear key, or says why there is none.
    fn with_linear_key<T>(
        &self,
        ask: impl FnOnce(&secrecy::SecretString) -> crate::Result<T>,
    ) -> Result<T, Arc<Error>> {
        match &self.linear_key {
            LinearKey::Found(key) => ask(key).map_err(Arc::new),
            LinearKey::Missing => Err(Arc::new(Error::LinearKeyMissing)),
            LinearKey::Unreadable(error) => Err(error.clone()),
        }
    }
}
