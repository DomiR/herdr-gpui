//! Enabling, restoring, and stopping a link without owning the agent's lifetime.

use super::*;
use crate::doc_view::{Changed, Docs, State};
use herdr_doc::Origin;

impl Links {
    pub(in crate::doc_link) fn enable(
        &mut self,
        request: Enable,
        cx: &mut Context<Self>,
    ) -> Result<()> {
        super::super::window::host(&request.target)?;
        let path = request.buffer.read(cx).path().to_owned();
        if !crate::doc_view::is_markdown(&path) {
            return Err(Error::Document(crate::doc_view::Error::NotMarkdown));
        }
        let version = request
            .buffer
            .read(cx)
            .document()
            .map(|doc| doc.version())
            .ok_or(Error::Document(crate::doc_view::Error::Loading))?;
        if self
            .entries
            .iter()
            .any(|link| link.path == path && link.mode != Mode::Off)
        {
            return Ok(());
        }
        if self
            .entries
            .iter()
            .any(|link| link.path == path && (link.job.is_some() || link.off))
        {
            return Err(Error::Busy);
        }
        if self
            .entries
            .iter()
            .filter(|link| link.mode != Mode::Off)
            .count()
            >= super::super::MAX_LINKS
        {
            return Err(Error::TooMany {
                limit: super::super::MAX_LINKS,
            });
        }
        let now = (self.now)();
        let live = self.index(&path).and_then(|index| {
            let saved = &self.entries[index];
            let mut record = saved.record.clone()?;
            let observed = if record.scope == request.scope {
                (now, request.snapshot.clone())
            } else {
                saved.snapshot.clone()?
            };
            let agent = observed
                .1
                .agents
                .iter()
                .find(|agent| agent.name.as_deref() == Some(record.agent_name.as_str()))?;
            record.enabled = true;
            record.pane_id.clone_from(&agent.pane_id);
            record.tab_id.clone_from(&agent.tab_id);
            Some((record, observed))
        });
        if let Some(index) = self.index(&path) {
            self.entries.remove(index);
        }
        self.sequence += 1;
        let mut link = Link::new(
            path.clone(),
            request.scope,
            request.target,
            request.workspace_id,
        );
        link.control = Some(Control {
            kind: ControlKind::Start,
            retry_at: now,
            version,
        });
        link.mode = Mode::Starting;
        link.push = Some(Push::new(version, self.config.debounce()));
        link.reveal = request.reveal;
        if let Some((record, observed)) = live {
            // A second editor may belong to another session. Reusing its
            // conductor preserves the saved owner instead of moving the link.
            link.scope = record.scope.clone();
            link.target = record
                .session
                .as_ref()
                .map_or(ConnectTarget::Local, |name| ConnectTarget::Session {
                    name: name.clone(),
                    development: false,
                });
            link.workspace_id = record.workspace_id.clone();
            link.last_status = observed
                .1
                .agents
                .iter()
                .find(|agent| agent.name.as_deref() == Some(record.agent_name.as_str()))
                .map(|agent| agent.agent_status);
            link.reveal = link.reveal.filter(|reveal| {
                reveal.scope == record.scope && reveal.workspace == record.workspace_id
            });
            link.record = Some(record);
            link.seen = true;
            link.snapshot = Some(observed);
        } else {
            link.snapshot = Some((now, request.snapshot));
        }
        link.used = self.sequence;
        link.version = Some(version);
        self.attach(&mut link, request.buffer, cx);
        self.entries.push(link);
        self.save(cx);
        self.pump(cx);
        cx.notify();
        Ok(())
    }

    pub(super) fn restore(&mut self, records: Vec<Record>, cx: &mut Context<Self>) {
        for record in records {
            self.sequence += 1;
            let target = record
                .session
                .as_ref()
                .map_or(ConnectTarget::Local, |name| ConnectTarget::Session {
                    name: name.clone(),
                    development: false,
                });
            let mut link = Link::new(
                record.path.clone(),
                record.scope.clone(),
                target,
                record.workspace_id.clone(),
            );
            if record.enabled {
                link.mode = Mode::Restoring;
                let buffer = Docs::open(&record.path, cx);
                self.attach(&mut link, buffer, cx);
            }
            link.record = Some(record);
            link.used = self.sequence;
            self.entries.push(link);
        }
        self.pump(cx);
    }

    fn attach(&self, link: &mut Link, buffer: Entity<DocBuffer>, cx: &mut Context<Self>) {
        // Docs uses lexical path keys. Aliases remain a known limit: never
        // canonicalize just the link and silently subscribe to another buffer.
        let path = link.path.clone();
        let changed = cx.subscribe(&buffer, move |this, _, event: &Changed, cx| {
            this.changed(&path, &event.by, (this.now)(), cx);
        });
        let observed = cx.observe(&buffer, |this, _, cx| {
            this.pump(cx);
        });
        link.buffer = Some(buffer);
        link.subscriptions = vec![changed, observed];
    }

    pub(crate) fn disable(&mut self, path: &Path, cx: &mut Context<Self>) {
        let Some(index) = self.index(path) else {
            return;
        };
        if self.entries[index].mode == Mode::Off {
            return;
        }
        self.stop(index, true, cx);
        self.pump(cx);
    }

    pub(super) fn stop(&mut self, index: usize, send_off: bool, cx: &mut Context<Self>) {
        self.sequence += 1;
        let link = &mut self.entries[index];
        link.generation = link.generation.wrapping_add(1);
        if let Some(job) = &link.job {
            job.cancelled.store(true, Ordering::Release);
        }
        let current = link
            .buffer
            .as_ref()
            .and_then(|buffer| buffer.read(cx).document())
            .map(|doc| doc.version());
        link.acked = link.push.as_ref().map(Push::acked).or(link.acked);
        link.off_range = current.map(|to| Range {
            from: link.acked.unwrap_or(to),
            to,
        });
        link.control = None;
        link.mode = Mode::Off;
        link.not_ready = false;
        link.off = send_off;
        link.push = None;
        link.retry_at = None;
        link.reveal = None;
        link.timer = None;
        link.used = self.sequence;
        if let Some(record) = &mut link.record {
            record.enabled = false;
        }
        self.save(cx);
        cx.notify();
    }

    pub(super) fn changed(
        &mut self,
        path: &Path,
        origin: &Origin,
        now: Instant,
        cx: &mut Context<Self>,
    ) {
        let Some(index) = self.index(path) else {
            return;
        };
        let link = &mut self.entries[index];
        let Some(version) = link
            .buffer
            .as_ref()
            .and_then(|buffer| buffer.read(cx).document())
            .map(|doc| doc.version())
        else {
            return;
        };
        link.version = Some(version);
        if let Some(push) = &mut link.push {
            let own = matches!(origin, Origin::Agent(author)
                if link.record.as_ref().is_some_and(|record| author.pane == record.pane_id));
            push.changed(version, own, now);
        }
        self.pump(cx);
    }

    pub(super) fn pump(&mut self, cx: &mut Context<Self>) {
        if self.quitting {
            return;
        }
        for index in 0..self.entries.len() {
            let link = &mut self.entries[index];
            if let Some(buffer) = &link.buffer {
                let buffer = buffer.read(cx);
                if let Some(doc) = buffer.document() {
                    link.version = Some(doc.version());
                } else if matches!(buffer.state(), State::Failed(_)) && link.mode != Mode::Off {
                    link.error = Some(Arc::new(Error::DocumentUnavailable {
                        path: link.path.clone(),
                    }));
                    self.stop(index, false, cx);
                    continue;
                }
            }
            self.restore_ready(index, cx);
            if self.entries[index].mode == Mode::Off && self.entries[index].job.is_none() {
                if self.entries[index].record.is_none() {
                    self.entries[index].off = false;
                }
                if !self.entries[index].off {
                    self.entries[index].buffer = None;
                    self.entries[index].subscriptions.clear();
                }
            }
            self.arm(index, cx);
        }
        // At most eight host jobs, including cancelled jobs still unwinding.
        // Enabled links waiting for a slot are visited again on completion.
        for index in 0..self.entries.len() {
            if self
                .entries
                .iter()
                .filter(|link| link.job.is_some())
                .count()
                >= super::super::MAX_LINKS
            {
                break;
            }
            self.dispatch(index, (self.now)(), cx);
        }
        self.trim();
    }

    fn arm(&mut self, index: usize, cx: &mut Context<Self>) {
        let link = &mut self.entries[index];
        if link.job.is_some() || matches!(link.mode, Mode::Off | Mode::Restoring) {
            link.timer = None;
            return;
        }
        let deadline = link
            .control
            .as_ref()
            .map(|control| control.retry_at)
            .or_else(|| link.push.as_ref().and_then(Push::deadline));
        let Some(deadline) =
            deadline.map(|deadline| link.retry_at.map_or(deadline, |retry| deadline.max(retry)))
        else {
            link.timer = None;
            return;
        };
        let now = (self.now)();
        if deadline <= now {
            link.timer = None;
            return;
        }
        if link
            .timer
            .as_ref()
            .is_some_and(|timer| timer.deadline == deadline)
        {
            return;
        }
        let path = link.path.clone();
        let generation = link.generation;
        let task = cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(deadline.saturating_duration_since(now))
                .await;
            let _ = this.update(cx, |this, cx| {
                let Some(index) = this.index(&path) else {
                    return;
                };
                let link = &mut this.entries[index];
                if link.generation != generation
                    || link
                        .timer
                        .as_ref()
                        .is_none_or(|timer| timer.deadline != deadline)
                {
                    return;
                }
                // A fired task is no longer armed, even if the clock says to wait longer.
                link.timer = None;
                this.pump(cx);
            });
        });
        link.timer = Some(Timer {
            _task: task,
            deadline,
        });
    }

    fn trim(&mut self) {
        while self.entries.len() > super::super::MAX_LINKS {
            let oldest = self
                .entries
                .iter()
                .enumerate()
                .filter(|(_, link)| link.mode == Mode::Off && link.job.is_none() && !link.off)
                .min_by_key(|(_, link)| link.used)
                .map(|(index, _)| index);
            let Some(index) = oldest else { break };
            self.entries.remove(index);
        }
    }
}

impl Link {
    fn new(path: PathBuf, scope: Scope, target: ConnectTarget, workspace_id: String) -> Self {
        Self {
            acked: None,
            buffer: None,
            control: None,
            error: None,
            failures: 0,
            generation: 0,
            job: None,
            last_status: None,
            mode: Mode::Off,
            not_ready: false,
            off: false,
            off_range: None,
            path,
            push: None,
            record: None,
            retry_at: None,
            reveal: None,
            scope,
            seen: false,
            snapshot: None,
            subscriptions: Vec::new(),
            target,
            timer: None,
            used: 0,
            version: None,
            workspace_id,
        }
    }
}
