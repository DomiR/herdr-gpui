//! Scope-qualified daemon snapshots and the first snapshot after restoration.

use super::*;

impl Links {
    pub(in crate::doc_link) fn observe_endpoint(
        &mut self,
        scope: &Scope,
        online: Option<Instant>,
        snapshot: Option<&Arc<ClientShellSnapshot>>,
        cx: &mut Context<Self>,
    ) {
        let (Some(online), Some(snapshot)) = (online, snapshot) else {
            // A disconnected connection can retain its last picture.
            return;
        };
        let mut pump = false;
        let mut save = false;
        for index in 0..self.entries.len() {
            let link = &mut self.entries[index];
            if &link.scope != scope {
                continue;
            }
            if let Some((previous_online, previous)) = &link.snapshot {
                if previous.boot_id == snapshot.boot_id && previous.revision >= snapshot.revision {
                    continue;
                }
                if previous.boot_id != snapshot.boot_id && online < *previous_online {
                    continue;
                }
            }
            link.snapshot = Some((online, snapshot.clone()));
            if link.mode == Mode::Off {
                continue;
            }
            pump |= link.mode == Mode::Restoring;
            let Some(record) = &mut link.record else {
                continue;
            };
            let agent = snapshot
                .agents
                .iter()
                .find(|agent| agent.name.as_deref() == Some(record.agent_name.as_str()));
            let Some(agent) = agent else {
                if link.seen || link.mode == Mode::Restoring {
                    self.stop(index, false, cx);
                    pump = true;
                }
                continue;
            };
            link.seen = true;
            if record.pane_id != agent.pane_id || record.tab_id != agent.tab_id {
                record.pane_id.clone_from(&agent.pane_id);
                record.tab_id.clone_from(&agent.tab_id);
                save = true;
            }
            let ready = matches!(
                agent.agent_status,
                AgentStatus::Idle | AgentStatus::Done | AgentStatus::Working
            );
            let was_unready = matches!(
                link.last_status,
                Some(AgentStatus::Blocked | AgentStatus::Unknown)
            );
            if link.not_ready && ready && was_unready {
                pump = true;
                link.retry_at = None;
                if let Some(control) = &mut link.control {
                    control.retry_at = (self.now)();
                }
                if let Some(push) = &mut link.push {
                    push.ready_again();
                }
            }
            link.last_status = Some(agent.agent_status);
        }
        if save {
            self.save(cx);
            cx.notify();
        }
        if pump {
            self.pump(cx);
        }
    }

    pub(super) fn restore_ready(&mut self, index: usize, cx: &mut Context<Self>) {
        let link = &mut self.entries[index];
        if link.mode != Mode::Restoring || link.control.is_some() {
            return;
        }
        let (Some((_, snapshot)), Some(record)) = (&link.snapshot, &mut link.record) else {
            return;
        };
        let Some(agent) = snapshot
            .agents
            .iter()
            .find(|agent| agent.name.as_deref() == Some(record.agent_name.as_str()))
        else {
            self.stop(index, false, cx);
            return;
        };
        let Some(version) = link.version else { return };
        link.seen = true;
        link.last_status = Some(agent.agent_status);
        record.pane_id.clone_from(&agent.pane_id);
        record.tab_id.clone_from(&agent.tab_id);
        link.push = Some(Push::new(version, self.config.debounce()));
        link.control = Some(Control {
            kind: ControlKind::Resync,
            retry_at: (self.now)(),
            version,
        });
        self.save(cx);
    }
}
