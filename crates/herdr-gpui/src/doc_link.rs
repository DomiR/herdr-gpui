//! Document agent links: scheduling, host jobs, app lifetime, and presentation.

pub(crate) mod error;
mod job;
mod messages;
mod model;
mod names;
mod presentation;
mod runtime;
mod startup;
mod status;
mod store;
pub(crate) mod window;

#[cfg(test)]
mod tests;

pub(crate) use error::{AgentCommandError, Error, JobStep, RecordError, Result, UnsupportedReason};
pub(crate) use job::{Delivery, Launch, Launched, Step, launch, prompt};
pub(crate) use messages::{change, off, resync, start};
pub(crate) use model::{Push, Range};
pub(crate) use names::{agent_name, tab_label, valid_agent_name};
pub(crate) use presentation::presentation;
pub(crate) use runtime::{Links, links};
pub(crate) use startup::Startup;
pub(crate) use status::{LinkStatus, Mode, Pending};
pub(crate) use store::{FILE, MAX_LINKS, Record, load, save};
