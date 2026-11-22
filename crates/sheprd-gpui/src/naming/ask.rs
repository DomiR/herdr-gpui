//! [`super::Way::Agent`]: the agent working in the subject is asked in its
//! own conversation, and its answer read back. The window polls rather than
//! holding a worker for the wait: an agent may finish other work first.

use super::{
    Error,
    work::{self, Terminal},
};
use crate::SheprdWindow;
use gpui::Context;
use sheprd_chat_protocol::{
    Command,
    naming::{Answer, answer, terminal_answer},
};
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

/// How long an agent may take to answer, long enough for a busy one to
/// finish first, bounded so the dialog does not wait forever.
const ASK_WAIT: Duration = Duration::from_secs(180);
/// How often the answer is looked for.
const POLL: Duration = Duration::from_secs(2);

impl SheprdWindow {
    /// Sends `prompt` to the chat the window keeps under `key` and fills the
    /// dialog with the name its turn answers.
    pub(super) fn ask_chat_for_name(
        &mut self,
        request: u64,
        key: String,
        agent: String,
        prompt: String,
        cx: &mut Context<Self>,
    ) {
        let Some(pane) = self.chats.pane_mut(&key) else {
            self.finish_naming(request, Err(Error::Gone), cx);
            return;
        };
        // Only this prompt's answer counts: an earlier identical one that
        // failed would otherwise answer a retry still queued.
        let after = pane
            .session
            .transcript()
            .entries
            .last()
            .map(|entry| entry.id);
        let origin = Some(pane.target.origin());
        let sent = pane.send(Command::Prompt {
            attachments: Vec::new(),
            origin,
            text: prompt.clone(),
        });
        if let Err(error) = sent {
            self.finish_naming(request, Err(error.into()), cx);
            return;
        }
        let deadline = Instant::now() + ASK_WAIT;
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(POLL).await;
                let found = this.update(cx, |this, _| {
                    this.naming_waits_for(request).then(|| {
                        this.chats
                            .pane(&key)
                            .map(|pane| answer(&prompt, pane.session.transcript(), after))
                    })
                });
                // The window or the dialog is gone.
                let Ok(Some(found)) = found else {
                    return;
                };
                let result = match found {
                    None => Err(Error::Gone),
                    Some(Some(Answer::Name(name))) => Ok(name),
                    Some(Some(Answer::NoName)) => Err(Error::NoName { agent }),
                    Some(None) if Instant::now() < deadline => continue,
                    Some(None) => Err(Error::TimedOut { agent }),
                };
                this.update(cx, |this, cx| this.finish_naming(request, result, cx))
                    .ok();
                return;
            }
        })
        .detach();
    }

    /// Types `prompt` into `terminal`'s agent and fills the dialog with the
    /// name it answers, once the same answer reads twice: a reply still
    /// streaming may show only its first words.
    pub(super) fn ask_terminal_for_name(
        &mut self,
        request: u64,
        terminal: Terminal,
        prompt: String,
        cancelled: Arc<AtomicBool>,
        cx: &mut Context<Self>,
    ) {
        let agent = terminal.agent();
        cx.spawn(async move |this, cx| {
            let executor = cx.background_executor().clone();
            let result = async {
                let sending = terminal.clone();
                executor
                    .spawn(async move { work::prompt(&sending, &prompt) })
                    .await?;
                let deadline = Instant::now() + ASK_WAIT;
                let mut last = None;
                loop {
                    executor.timer(POLL).await;
                    if cancelled.load(Ordering::Acquire) {
                        return Err(Error::Gone);
                    }
                    let (reading, flag) = (terminal.clone(), cancelled.clone());
                    let text = executor
                        .spawn(async move { work::read(&reading, &flag) })
                        .await?;
                    let found = terminal_answer(&text, request);
                    if let Some(name) = found.as_ref().filter(|_| found == last) {
                        return Ok(name.clone());
                    }
                    if Instant::now() >= deadline {
                        return Err(Error::TimedOut { agent });
                    }
                    last = found;
                }
            }
            .await;
            this.update(cx, |this, cx| this.finish_naming(request, result, cx))
                .ok();
        })
        .detach();
    }
}
