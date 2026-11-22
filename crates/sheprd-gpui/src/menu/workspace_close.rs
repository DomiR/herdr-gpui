//! A fresh, cancellable preflight owned by one close dialog. Unknown status
//! requires the same explicit consent as work that has not been published.
// i18n: migrated
use std::path::Path;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc,
};
use std::time::{Duration, Instant};

use gpui::App;
use sheprd_client::protocol::{ClientShellSnapshot, ClientShellWorktree};

use super::workspace::{Source, WorkspaceTarget};
use crate::i18n::fl;

#[derive(Clone, Debug, PartialEq, Eq)]
struct Checkout {
    id: String,
    worktree: Option<ClientShellWorktree>,
    branch: Option<String>,
}

#[cfg(test)]
mod tests;

/// What the user types to close anyway, the same word in every language.
const CONSENT: &str = "close"; // i18n: data

/// The Git line while the check runs.
pub(super) fn checking() -> String {
    fl!("workspace-close-checking")
}

/// The repository and branch the worker read from the folder itself.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Seen {
    key: String,
    branch: String,
}

impl Seen {
    /// Whether `dir` is still that repository on that branch, read afresh.
    // shortcut: two quick Git reads on the UI thread under a one second bound
    // at submission; move them to a background task if they ever show lag.
    pub(super) fn is_current(&self, dir: &str) -> bool {
        sheprd_vcs::git::identity(dir, Instant::now() + Duration::from_secs(1), &|| false)
            .ok()
            .flatten()
            .is_some_and(|now| {
                now.key == self.key && now.branch.as_deref() == Some(self.branch.as_str())
            })
    }
}

#[derive(Default)]
pub(super) struct Report {
    pub dirty: bool,
    pub unpushed: bool,
    pub unknown: bool,
    /// Set for an app-kept folder whose identity the check could read.
    pub seen: Option<Seen>,
}

/// What one check probes: a Herdr 1 checkout the daemon described, or an
/// app-kept folder the worker identifies itself.
enum Probe {
    Input(sheprd_github::Result<crate::pull_request::Input>),
    Folder(sheprd_github::Result<String>),
}

/// An app-kept folder's status. Its repository and branch come from the
/// folder now, never from the session's slower identity cache.
fn folder_status(
    dir: &str,
    deadline: Instant,
    cancelled: &impl Fn() -> bool,
) -> crate::Result<(bool, bool, Seen)> {
    use sheprd_github::pull_request::Error as Pull;
    if !Path::new(dir).is_absolute() {
        return Err(sheprd_github::Error::from(Pull::AbsolutePath).into());
    }
    let identity = sheprd_vcs::git::identity(dir, deadline, cancelled)?
        .ok_or_else(|| crate::Error::from(sheprd_github::Error::from(Pull::Metadata)))?;
    let branch = identity
        .branch
        .ok_or_else(|| crate::Error::from(sheprd_github::Error::from(Pull::Branch)))?;
    let input = crate::pull_request::Input {
        checkout: Some(dir.to_owned()),
        repo_key: Some(identity.key.clone()),
        branch: branch.clone(),
    };
    let (dirty, unpushed) = crate::git::close_status(&input, deadline, cancelled)?;
    Ok((
        dirty,
        unpushed,
        Seen {
            key: identity.key,
            branch,
        },
    ))
}

impl Report {
    pub fn needs_consent(&self) -> bool {
        self.dirty || self.unpushed || self.unknown
    }
}

pub(super) struct CloseCheck {
    checkouts: Vec<Checkout>,
    receiver: mpsc::Receiver<Report>,
    cancelled: Arc<AtomicBool>,
    pub report: Option<Report>,
}

impl Drop for CloseCheck {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }
}

fn checkouts(snapshot: &ClientShellSnapshot, target: &WorkspaceTarget) -> Vec<Checkout> {
    snapshot
        .workspaces
        .iter()
        .filter(|workspace| target.close_members.contains(&workspace.workspace_id))
        .map(|workspace| Checkout {
            id: workspace.workspace_id.clone(),
            worktree: workspace.worktree.clone(),
            branch: workspace.branch.clone(),
        })
        .collect()
}

impl CloseCheck {
    #[cfg(test)]
    pub(super) fn fixture(
        snapshot: &ClientShellSnapshot,
        target: &WorkspaceTarget,
        report: Option<Report>,
    ) -> Self {
        let (_, receiver) = mpsc::sync_channel(1);
        Self {
            checkouts: checkouts(snapshot, target),
            receiver,
            cancelled: Arc::new(AtomicBool::new(false)),
            report,
        }
    }

    pub fn start(
        snapshot: &ClientShellSnapshot,
        target: &WorkspaceTarget,
        local: bool,
        cx: &App,
    ) -> Self {
        let checkouts = checkouts(snapshot, target);
        let inputs: Vec<_> = checkouts
            .iter()
            .take(128)
            .map(|checkout| {
                Probe::Input(crate::pull_request::repository_input(
                    checkout.worktree.as_ref(),
                    checkout.branch.as_deref(),
                ))
            })
            .collect();
        Self::spawn(checkouts, inputs, local, cx)
    }

    /// App-kept folders belong to the selected host, independently of any
    /// Herdr 1 connection. The worker verifies their repository and branch.
    pub fn start_kept(target: &WorkspaceTarget, local: bool, cx: &App) -> Self {
        let probe = match &target.source {
            Source::Directory(cwd) => Probe::Folder(Ok(cwd.clone())),
            _ => Probe::Folder(Err(sheprd_github::pull_request::Error::Metadata.into())),
        };
        Self::spawn(Vec::new(), vec![probe], local, cx)
    }

    fn spawn(checkouts: Vec<Checkout>, probes: Vec<Probe>, local: bool, cx: &App) -> Self {
        let mut report = Report {
            unknown: checkouts.len() > 128,
            ..Report::default()
        };
        let (sender, receiver) = mpsc::sync_channel(1);
        let cancelled = Arc::new(AtomicBool::new(false));
        let cancellation = cancelled.clone();
        cx.background_executor()
            .spawn(async move {
                let deadline = Instant::now() + Duration::from_secs(15);
                let cancelled = || cancellation.load(Ordering::Relaxed);
                for probe in probes {
                    if cancelled() {
                        return;
                    }
                    let status = match probe {
                        Probe::Input(input) => {
                            input.map_err(crate::Error::from).and_then(|input| {
                                if !local {
                                    return Err(
                                        sheprd_github::pull_request::Error::UntrustedEndpoint
                                            .into(),
                                    );
                                }
                                let (dirty, unpushed) =
                                    crate::git::close_status(&input, deadline, &cancelled)?;
                                Ok((dirty, unpushed, None))
                            })
                        }
                        Probe::Folder(dir) => dir.map_err(crate::Error::from).and_then(|dir| {
                            if !local {
                                return Err(
                                    sheprd_github::pull_request::Error::UntrustedEndpoint.into()
                                );
                            }
                            let (dirty, unpushed, seen) =
                                folder_status(&dir, deadline, &cancelled)?;
                            Ok((dirty, unpushed, Some(seen)))
                        }),
                    };
                    match status {
                        Ok((dirty, unpushed, seen)) => {
                            report.dirty |= dirty;
                            report.unpushed |= unpushed;
                            report.seen = seen;
                        }
                        Err(_) => report.unknown = true,
                    }
                }
                let _ = sender.send(report);
            })
            .detach();
        Self {
            checkouts,
            receiver,
            cancelled,
            report: None,
        }
    }

    pub fn poll(&mut self) -> bool {
        if self.report.is_some() {
            return false;
        }
        match self.receiver.try_recv() {
            Ok(report) => self.report = Some(report),
            Err(mpsc::TryRecvError::Empty) => return false,
            Err(mpsc::TryRecvError::Disconnected) => {
                self.report = Some(Report {
                    unknown: true,
                    ..Report::default()
                })
            }
        }
        true
    }

    pub fn current(&self, snapshot: &ClientShellSnapshot, target: &WorkspaceTarget) -> bool {
        self.checkouts == checkouts(snapshot, target)
    }

    /// Whether the close may go without the dialog once
    /// `confirm_close_workspace` is off: the check finished without finding
    /// uncommitted files or unpushed commits. Unverifiable status does not
    /// hold it, since a workspace outside a Herdr worktree is never
    /// verifiable and turning confirmation off asked not to be stopped.
    pub fn skippable(&self, confirm: bool) -> bool {
        !confirm
            && self
                .report
                .as_ref()
                .is_some_and(|report| !report.dirty && !report.unpushed)
    }

    /// Whether the close may be sent, with `text` typed as consent.
    pub fn allows(&self, text: &str, confirm: bool) -> bool {
        self.ready(text) || self.skippable(confirm)
    }

    /// The dialog's Git line. It asks for typed consent only while the
    /// close needs it, since the input shows only then.
    pub fn status(&self, confirm: bool) -> String {
        let Some(report) = &self.report else {
            return checking();
        };
        let mut warnings = Vec::new();
        if report.dirty {
            warnings.push(fl!("workspace-close-dirty"));
        }
        if report.unpushed {
            warnings.push(fl!("workspace-close-unpushed"));
        }
        if report.unknown {
            warnings.push(fl!("workspace-close-unknown"));
        }
        if !report.needs_consent() {
            warnings.push(fl!("workspace-close-clean"));
        } else if !self.skippable(confirm) {
            warnings.push(fl!("workspace-close-consent", word = CONSENT));
        }
        warnings.join(" ")
    }

    pub fn ready(&self, text: &str) -> bool {
        self.report
            .as_ref()
            .is_some_and(|report| !report.needs_consent() || text == CONSENT)
    }
}
