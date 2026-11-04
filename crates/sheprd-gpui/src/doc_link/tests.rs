#![allow(clippy::unwrap_used, clippy::expect_used)]

mod job;
mod messages;
mod model;
mod names;
mod retention;
mod store;

use super::{
    error::{Error, JobStep, RecordError, Result},
    job::{Launch, Launched, PromptOutcome, PromptRefusal, Step, launch_core, prompt_core},
    messages::*,
    model::*,
    names::*,
    store::*,
};
use sheprd_doc::Version;
use std::{
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

fn version(number: u64) -> Version {
    Version(number)
}

fn path() -> PathBuf {
    PathBuf::from("/tmp/notes folder/O'Reilly.md")
}

fn base() -> Instant {
    Instant::now()
}
