//! One cancellable host job and one pending control message per link.

use super::*;
use sheprd_doc::Origin;

#[derive(Clone, Copy)]
enum Operation {
    // keep-sorted start
    Change,
    Control,
    Launch,
    Off,
    // keep-sorted end
}

enum Outcome {
    // keep-sorted start
    Launch(LaunchResult),
    Prompt(Result<Delivery>),
    // keep-sorted end
}

impl Links {
    pub(super) fn dispatch(&mut self, index: usize, now: Instant, cx: &mut Context<Self>) {
        let link = &mut self.entries[index];
        if link.job.is_some() {
            return;
        }
        if link.retry_at.is_some_and(|retry| now < retry) {
            return;
        }
        let operation = if link.mode == Mode::Off {
            if !link.off || link.record.is_none() {
                return;
            }
            Operation::Off
        } else if let Some(control) = &link.control {
            if now < control.retry_at {
                return;
            }
            if link.record.is_none() {
                Operation::Launch
            } else {
                Operation::Control
            }
        } else if link.mode == Mode::Restoring {
            return;
        } else {
            Operation::Change
        };
        let Some(buffer) = &link.buffer else { return };
        let Some(doc) = buffer.read(cx).document() else {
            return;
        };
        let version = doc.version();
        let text = match operation {
            Operation::Off => {
                let range = link.off_range.unwrap_or(Range {
                    from: link.acked.unwrap_or(version),
                    to: version,
                });
                super::super::off(&link.path, range.from, range.to)
            }
            Operation::Launch | Operation::Control => {
                let Some(control) = &link.control else { return };
                match control.kind {
                    ControlKind::Start => super::super::start(&link.path, control.version),
                    ControlKind::Resync => super::super::resync(&link.path, control.version),
                }
            }
            Operation::Change => {
                let Some(push) = &mut link.push else { return };
                if push.in_flight() || push.deadline().is_none_or(|deadline| now < deadline) {
                    return;
                }
                // doc diff and doc watch already compute this on the UI thread;
                // doing it once per debounce interval replaces their more frequent work.
                let changes = doc.changes(push.acked(), None);
                let own = |origin: &Origin| {
                    matches!(origin, Origin::Agent(author)
                    if link.record.as_ref().is_some_and(|record| author.pane == record.pane_id))
                };
                let eligible = changes.as_ref().is_none_or(|changes| {
                    (changes.by.is_empty() || !changes.by.iter().all(own))
                        && changes.diff.as_deref() != Some("")
                });
                let Some(range) = push.take(now, version, eligible) else {
                    return;
                };
                super::super::change(
                    &link.path,
                    range,
                    changes.as_ref().and_then(|changes| changes.diff.as_deref()),
                )
            }
        };
        let host = match super::super::window::host(&link.target) {
            Ok(host) => host,
            Err(error) => {
                link.error = Some(Arc::new(error));
                self.stop(index, false, cx);
                return;
            }
        };
        let path = link.path.clone();
        let generation = link.generation;
        let cancelled = Arc::new(AtomicBool::new(false));
        let cancel = cancelled.clone();
        let task = match operation {
            Operation::Launch => {
                let config = self.config.clone();
                let path = path.clone();
                let workspace = link.workspace_id.clone();
                let launch = self.jobs.launch.clone();
                cx.background_executor().spawn(async move {
                    let label = super::super::tab_label(&path);
                    let args = Launch {
                        args: config.args(),
                        cwd: path.parent().unwrap_or(Path::new(".")),
                        kind: config.kind(),
                        label: &label,
                        prompt: &text,
                        workspace_id: &workspace,
                    };
                    Outcome::Launch(launch(&host, &args, &path, &cancel))
                })
            }
            _ => {
                let Some(record) = &link.record else { return };
                let name = record.agent_name.clone();
                let prompt = self.jobs.prompt.clone();
                cx.background_executor()
                    .spawn(async move { Outcome::Prompt(prompt(&host, &name, &text, &cancel)) })
            }
        };
        link.timer = None;
        link.job = Some(Flight {
            _task: cx.spawn(async move |this, cx| {
                let outcome = task.await;
                let _ = this.update(cx, |this, cx| {
                    this.completed(&path, generation, operation, outcome, cx);
                });
            }),
            cancelled,
        });
        cx.notify();
    }

    fn completed(
        &mut self,
        path: &Path,
        generation: u64,
        operation: Operation,
        outcome: Outcome,
        cx: &mut Context<Self>,
    ) {
        let Some(index) = self.index(path) else {
            return;
        };
        let link = &mut self.entries[index];
        link.job = None;
        if self.quitting {
            return;
        }
        let stale = generation != link.generation;
        match outcome {
            Outcome::Launch(LaunchResult { result, started }) => {
                let launched = result.as_ref().ok().cloned().or(started);
                if let Some(agent) = launched {
                    link.record = Some(Record {
                        agent_name: agent.agent_name,
                        enabled: !stale && link.mode != Mode::Off,
                        pane_id: agent.pane_id,
                        path: link.path.clone(),
                        scope: link.scope.clone(),
                        session: match &link.target {
                            ConnectTarget::Session { name, .. } => Some(name.clone()),
                            _ => None,
                        },
                        tab_id: agent.tab_id,
                        workspace_id: link.workspace_id.clone(),
                    });
                    if let Some((_, snapshot)) = &link.snapshot
                        && let Some(record) = &mut link.record
                        && let Some(agent) = snapshot
                            .agents
                            .iter()
                            .find(|agent| agent.name.as_deref() == Some(record.agent_name.as_str()))
                    {
                        link.seen = true;
                        link.last_status = Some(agent.agent_status);
                        record.pane_id.clone_from(&agent.pane_id);
                        record.tab_id.clone_from(&agent.tab_id);
                    }
                }
                if !stale {
                    match result {
                        Ok(_) => {
                            link.control = None;
                            link.error = None;
                            link.failures = 0;
                            link.mode = Mode::On;
                            link.not_ready = false;
                        }
                        Err(error) => {
                            link.not_ready = matches!(error, Error::NotReady { .. });
                            link.error = Some(Arc::new(error));
                            if link.record.is_some() {
                                link.mode = Mode::Paused;
                                link.failed((self.now)(), self.config.debounce());
                            } else {
                                self.stop(index, false, cx);
                            }
                        }
                    }
                }
                self.save(cx);
            }
            Outcome::Prompt(result) if !stale => match operation {
                Operation::Off => {
                    link.off = false;
                }
                _ => match result {
                    Ok(Delivery::Sent) => {
                        if matches!(operation, Operation::Control) {
                            link.control = None;
                        } else if let Some(push) = &mut link.push {
                            push.sent();
                        }
                        link.error = None;
                        link.failures = 0;
                        link.mode = Mode::On;
                        link.not_ready = false;
                        link.retry_at = None;
                    }
                    Ok(Delivery::Gone) => self.stop(index, false, cx),
                    result => {
                        link.not_ready = matches!(result, Ok(Delivery::NotReady));
                        link.error = result.err().map(Arc::new);
                        link.mode = Mode::Paused;
                        link.failed((self.now)(), self.config.debounce());
                    }
                },
            },
            Outcome::Prompt(_) => {}
        }
        self.pump(cx);
        cx.notify();
    }
}

impl Link {
    fn failed(&mut self, now: Instant, debounce: Duration) {
        self.failures = self.failures.saturating_add(1);
        let multiplier = 1_u32 << self.failures.saturating_sub(1).min(31);
        let delay = debounce
            .saturating_mul(multiplier)
            .min(Duration::from_secs(300));
        self.retry_at = Some(now + delay);
        if let Some(control) = &mut self.control {
            control.retry_at = now + delay;
        } else if let Some(push) = &mut self.push {
            push.failed(now, delay);
        }
    }
}
