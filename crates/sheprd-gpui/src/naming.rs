//! A name for a workspace or tab from an agent, offered in their rename
//! dialogs as the phone offers one, two ways:
//!
//! - [`Way::Claude`]: Claude, run once on its own on this computer, reads
//!   the end of the chats and terminals there and answers a short title.
//!   Nothing is sent to them, so their sessions and context are untouched.
//! - [`Way::Agent`]: the agent working there is asked in its own
//!   conversation, a chat's or a terminal agent's, and its answer is read
//!   back from the transcript or the terminal's text. The question and the
//!   answer stay in that conversation.
//!
//! Either way the name only fills the dialog's field: the person sees it,
//! may change it, and renames as they would have. A dialog closed or asked
//! again drops the answer still on its way, and stops what was reading it.

mod ask;
mod error;
mod render;
mod sources;
mod work;

pub(crate) use error::Error;
pub(crate) use sheprd_chat_protocol::naming::Subject;

use crate::{SheprdWindow, i18n::errors::Localize};
use gpui::Context;
use sources::Asked;
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU64, Ordering},
};

/// Which way the agent is asked.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Way {
    /// Claude, run once on its own over what the subject shows.
    Claude,
    /// The agent working in the subject, in its own conversation.
    Agent,
}

/// The rename dialog whose field a name fills.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Field {
    /// A Herdr 1 tab's menu (`tab_menu`).
    Tab,
    /// A herdr-core tab's menu (`core_terminals::menu`).
    CoreTab,
    /// The workspace dialog, Herdr 1's or a kept workspace's.
    Workspace,
}

impl Field {
    fn subject(self) -> Subject {
        match self {
            Self::Tab | Self::CoreTab => Subject::Tab,
            Self::Workspace => Subject::Workspace,
        }
    }
}

/// The naming offered by an open rename dialog, and the request under way.
#[derive(Debug)]
pub(crate) struct Naming {
    field: Field,
    /// What the agent answering [`Way::Agent`] is called, when the subject
    /// has one.
    agent: Option<String>,
    running: Option<Running>,
    error: Option<String>,
}

/// A request on its way. Dropping it stops the work still reading for it.
#[derive(Debug)]
struct Running {
    way: Way,
    request: u64,
    cancelled: Arc<AtomicBool>,
}

impl Drop for Running {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Release);
    }
}

/// Numbers each request, so a late answer to one the dialog no longer
/// waits for is dropped, and a terminal's earlier answers are told from
/// this one's. Seeded from the clock, as a terminal can still show an ask
/// from the app's last run.
fn next_request() -> u64 {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let seed = || {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(1, |elapsed| elapsed.as_millis() as u64 % 1_000_000 * 1_000)
    };
    let _ = NEXT.compare_exchange(0, seed(), Ordering::AcqRel, Ordering::Acquire);
    NEXT.fetch_add(1, Ordering::AcqRel)
}

impl Naming {
    pub(crate) fn running(&self) -> Option<Way> {
        self.running.as_ref().map(|running| running.way)
    }

    /// What the agent [`Way::Agent`] asks is called, when there is one.
    pub(crate) fn agent(&self) -> Option<&str> {
        self.agent.as_deref()
    }

    /// Why the last request gave no name.
    pub(crate) fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }
}

impl SheprdWindow {
    /// Offers naming in the rename dialog for `field`, just opened.
    pub(crate) fn offer_naming(&mut self, field: Field) {
        let agent = self
            .naming_sources(field)
            .and_then(|sources| sources.asked_agent().map(str::to_owned));
        self.menu.naming = Some(Naming {
            field,
            agent,
            running: None,
            error: None,
        });
    }

    /// Asks for a name `way` for the open rename dialog's subject.
    pub(crate) fn start_naming(&mut self, way: Way, cx: &mut Context<Self>) {
        let Some(field) = self.menu.naming.as_ref().map(|naming| naming.field) else {
            return;
        };
        let request = next_request();
        let cancelled = Arc::new(AtomicBool::new(false));
        if let Some(naming) = &mut self.menu.naming {
            naming.error = None;
            // Replacing an earlier request stops it.
            naming.running = Some(Running {
                way,
                request,
                cancelled: cancelled.clone(),
            });
        }
        cx.notify();
        let current = self.naming_current(field, cx);
        let Some(sources) = self.naming_sources(field) else {
            self.finish_naming(request, Err(Error::Empty), cx);
            return;
        };
        let prompt = sheprd_chat_protocol::naming::ask_prompt(field.subject(), &current, request);
        match way {
            Way::Claude => {
                let work = cx
                    .background_executor()
                    .spawn(async move { work::ask_claude(sources.into_reading(), &cancelled) });
                self.naming_answer_from(request, work, cx);
            }
            Way::Agent => match sources.into_asked() {
                None => self.finish_naming(request, Err(Error::Empty), cx),
                Some(Asked::Chat { key, agent }) => {
                    self.ask_chat_for_name(request, key, agent, prompt, cx);
                }
                Some(Asked::Terminal(terminal)) => {
                    self.ask_terminal_for_name(request, terminal, prompt, cancelled, cx);
                }
            },
        }
    }

    /// Applies `work`'s answer to `request` once it comes.
    fn naming_answer_from(
        &mut self,
        request: u64,
        work: gpui::Task<Result<String, Error>>,
        cx: &mut Context<Self>,
    ) {
        cx.spawn(async move |this, cx| {
            let answer = work.await;
            this.update(cx, |this, cx| this.finish_naming(request, answer, cx))
                .ok();
        })
        .detach();
    }

    /// Whether the open dialog still waits for `request`.
    fn naming_waits_for(&self, request: u64) -> bool {
        self.menu
            .naming
            .as_ref()
            .and_then(|naming| naming.running.as_ref())
            .is_some_and(|running| running.request == request)
    }

    /// Fills the dialog's field with the name `request` found, or says why
    /// there is none; an answer the dialog no longer waits for is dropped.
    fn finish_naming(
        &mut self,
        request: u64,
        answer: Result<String, Error>,
        cx: &mut Context<Self>,
    ) {
        if !self.naming_waits_for(request) {
            return;
        }
        let Some(naming) = &mut self.menu.naming else {
            return;
        };
        naming.running = None;
        let field = naming.field;
        match answer {
            Ok(name) => self.fill_naming_field(field, &name, cx),
            Err(error) => naming.error = Some(error.localize()),
        }
        cx.notify();
    }

    /// What the dialog's field holds now: the name the agent is told.
    fn naming_current(&self, field: Field, cx: &gpui::App) -> String {
        match field {
            Field::Workspace => self
                .menu
                .input
                .as_ref()
                .map(|input| input.text.clone())
                .unwrap_or_default(),
            Field::Tab => self.tab_rename_text(cx).unwrap_or_default(),
            Field::CoreTab => self.core_rename_text(cx).unwrap_or_default(),
        }
    }

    /// Puts `name` in the dialog's field, selected so typing replaces it.
    fn fill_naming_field(&mut self, field: Field, name: &str, cx: &mut Context<Self>) {
        match field {
            Field::Workspace => {
                self.menu.input = Some(crate::dialog_input::DialogInput::new(name.to_owned()));
                self.menu.error = None;
            }
            Field::Tab => self.fill_tab_rename(name, cx),
            Field::CoreTab => self.fill_core_rename(name, cx),
        }
    }
}

#[cfg(test)]
impl SheprdWindow {
    /// Marks a request under way, as [`SheprdWindow::start_naming`] does
    /// before its work starts.
    pub(crate) fn begin_naming_for_test(&mut self, way: Way) -> u64 {
        let request = next_request();
        if let Some(naming) = &mut self.menu.naming {
            naming.running = Some(Running {
                way,
                request,
                cancelled: Arc::default(),
            });
        }
        request
    }

    pub(crate) fn finish_naming_for_test(
        &mut self,
        request: u64,
        answer: Result<String, Error>,
        cx: &mut Context<Self>,
    ) {
        self.finish_naming(request, answer, cx);
    }
}

#[cfg(test)]
mod tests;
