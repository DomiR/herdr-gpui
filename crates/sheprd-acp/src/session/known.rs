//! Settings shown before the agent reports them: a chat that starts or
//! switches to a built-in agent offers the models, thinking levels, and
//! modes that agent offered last ([`crate::known`]) while it comes up, and
//! a pick made meanwhile is shown at once and sent once its session opens.

use sheprd_chat_protocol::{
    Agent, Change, OptionKind, OptionValue, Rejection, Setting, Settings, Status, Transcript,
};

use super::{Session, Update};
use crate::known::{self, Known};

/// A setting picked before the agent's session opened.
#[derive(Debug, Eq, PartialEq)]
pub(super) enum Deferred {
    Mode(String),
    Option { id: String, value: OptionValue },
}

/// Fills `transcript` with what `agent` offered last, when known.
pub(super) fn seed(known: &Known, agent: Option<Agent>, transcript: &mut Transcript) {
    let Some(settings) = agent.and_then(|agent| known.get(&agent)) else {
        return;
    };
    transcript.modes.clone_from(&settings.modes);
    transcript.options.clone_from(&settings.options);
}

impl<R: FnMut(Update)> Session<R> {
    fn settings(&self) -> known::Settings {
        known::Settings {
            modes: self.transcript.modes.clone(),
            options: self.transcript.options.clone(),
        }
    }

    /// A new session opened: what it offers is what the agent shows next
    /// time, kept on disk when it changed.
    pub(super) fn learn(&mut self, loaded: bool) {
        let Some(agent) = self.config.harness.filter(|_| !loaded) else {
            return;
        };
        let settings = self.settings();
        let known = self.config.known.get(&agent);
        // Nothing offered forgets what was known, so the next start shows
        // no settings the agent dropped.
        if known == Some(&settings) || (known.is_none() && settings.is_empty()) {
            return;
        }
        if settings.is_empty() {
            self.config.known.remove(&agent);
        } else {
            self.config.known.insert(agent, settings.clone());
        }
        (self.report)(Update::Settings(agent, settings));
    }

    /// The agent switched away from: what it shows now, should the chat
    /// come back to it, which continues its session as it was left. Picks
    /// it never took do not follow the chat to the next agent.
    pub(super) fn keep_leaving(&mut self) {
        self.deferred.clear();
        let Some(agent) = self.config.harness.filter(|_| self.ready) else {
            return;
        };
        let settings = self.settings();
        if !settings.is_empty() {
            self.config.known.insert(agent, settings);
        }
    }

    /// Whether a pick waits for the session: the agent is still starting.
    fn starting(&self) -> bool {
        !self.ready && self.transcript.status == Status::Starting
    }

    pub(super) fn defer_mode(&mut self, mode: String) -> Result<(), Rejection> {
        let known = self
            .transcript
            .modes
            .as_ref()
            .is_some_and(|modes| modes.available.iter().any(|known| known.id == mode));
        if !self.starting() || !known {
            return Err(Rejection::NotReady);
        }
        self.deferred
            .retain(|deferred| !matches!(deferred, Deferred::Mode(_)));
        self.deferred.push(Deferred::Mode(mode.clone()));
        self.emit(Change::CurrentMode { id: mode });
        Ok(())
    }

    pub(super) fn defer_option(&mut self, id: String, value: OptionValue) -> Result<(), Rejection> {
        if !self.starting() {
            return Err(Rejection::NotReady);
        }
        let mut options = self.transcript.options.clone();
        let option = options
            .iter_mut()
            .find(|option| option.id == id && option.accepts(&value))
            .ok_or(Rejection::NotReady)?;
        match (&mut option.kind, &value) {
            (OptionKind::Select { current, .. }, OptionValue::Choice { id }) => {
                current.clone_from(id);
            }
            (OptionKind::Toggle { current }, OptionValue::Toggle { value }) => *current = *value,
            _ => return Err(Rejection::NotReady),
        }
        self.deferred.retain(
            |deferred| !matches!(deferred, Deferred::Option { id: picked, .. } if *picked == id),
        );
        self.deferred.push(Deferred::Option { id, value });
        self.emit(Change::Options { options });
        Ok(())
    }

    /// Hands the picks made while the agent started to the settings carried
    /// into its session, after those so the person's pick wins, to be sent
    /// one at a time like them; prompts wait for them too. A continued
    /// session takes them as well, though it keeps its own carried ones.
    /// One the session no longer offers is skipped there: the setting then
    /// shows the agent's own.
    pub(super) fn carry_deferred(&mut self) {
        let Some(agent) = self.config.harness else {
            self.deferred.clear();
            return;
        };
        if self.deferred.is_empty() {
            return;
        }
        let carried = self.config.settings.get_or_insert_with(|| Settings {
            agent: agent.name().to_owned(),
            ..Settings::default()
        });
        for deferred in std::mem::take(&mut self.deferred) {
            match deferred {
                Deferred::Mode(mode) => carried.mode = Some(mode),
                Deferred::Option { id, value } => {
                    carried.options.retain(|setting| setting.id != id);
                    carried.options.push(Setting { id, value });
                }
            }
        }
    }
}
