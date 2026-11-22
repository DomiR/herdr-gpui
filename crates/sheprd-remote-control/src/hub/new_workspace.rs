//! A phone's new workspace: in a folder, on a new worktree when a branch
//! is given, with a chat in it.
//!
//! A worktree of a repository Herdr 1 already shows is made by Herdr 1 from
//! that repository's workspace, as Sheprd's own new worktree is, so it
//! joins the repository's group in every client. Its chat still starts on
//! the core while one answers, in that workspace. Any other new workspace
//! goes to the core, which Sheprd keeps a workspace for, or to Herdr 1 when
//! no core answers.

use super::{
    ApiMethod, CreatedWorkspace, Error, Hub, Launcher, Layout, Result, command_line, core,
    created_tab, direct_command_line, on_core, place, sessions, tab_params,
};
use crate::native::changes::local::git;
use serde_json::{Value, json};
use sheprd_chat_protocol::Agent;
use std::path::{Path, PathBuf};

/// What a phone's new workspace asks for.
#[derive(Clone, Copy, Debug)]
pub(crate) struct NewWorkspace<'a> {
    pub(crate) agent: Agent,
    /// A new worktree on this branch, else the folder itself.
    pub(crate) branch: Option<&'a str>,
    pub(crate) cwd: &'a str,
    /// The Herdr 1 session this daemon follows by that name; the primary
    /// otherwise.
    pub(crate) session: Option<&'a str>,
    /// The name the person gave the workspace; else it is named for its
    /// folder or branch.
    pub(crate) title: Option<&'a str>,
}

impl Hub {
    /// Creates `new`, answering its workspace and its chat's pane. While a
    /// core answers the chat starts there.
    pub(crate) async fn create_workspace(&self, new: NewWorkspace<'_>) -> Result<CreatedWorkspace> {
        // As in [`Hub::start_chat`]: the program here cannot run in a
        // distribution, and a core would open the chat on this computer.
        if new.session.is_some_and(sessions::is_wsl) {
            return Err(Error::WslChat);
        }
        let launcher = self.launcher().await?;
        let core = self.core_for_work().await?;
        if let Some(branch) = new.branch {
            let session = new.session.filter(|name| self.sessions.get(name).is_some());
            let cwd = PathBuf::from(new.cwd);
            // Git, not a workspace's folder, says which repository `cwd`
            // is: a workspace's folder follows its shell elsewhere.
            let checkout = on_core(move || Ok(Checkout::of(&cwd))).await?;
            let repository = checkout.and_then(|checkout| {
                herdr_1_repository(&self.layout(), &checkout, session, |id| {
                    self.kept_workspace(id)
                })
            });
            if let Some(source) = repository {
                let worktree = Worktree {
                    base: source.base.as_deref(),
                    branch,
                    repository: &source.workspace,
                    title: new.title,
                };
                return self
                    .herdr_1_worktree(core, worktree, new.agent, &launcher)
                    .await;
            }
        }
        if let Some(core) = core {
            return self.core_workspace(core, new, &launcher).await;
        }
        self.herdr_1_workspace(new, &launcher).await
    }

    /// `new` on the core, in its unlisted group until Sheprd keeps a
    /// workspace for it: the core has no workspaces or worktrees, so a
    /// branch's checkout is made here.
    async fn core_workspace(
        &self,
        core: std::sync::Arc<core::Source>,
        new: NewWorkspace<'_>,
        launcher: &Launcher,
    ) -> Result<CreatedWorkspace> {
        // Herdr 2 keeps no Herdr 1 session: asked for a named one, the
        // workspace could not be where it was asked for.
        if new
            .session
            .is_some_and(|name| self.sessions.get(name).is_some())
        {
            return Err(Error::SessionOnHerdr2);
        }
        let (cwd, launcher, agent) = (new.cwd.to_owned(), launcher.clone(), new.agent);
        let (branch, root) = (new.branch.map(str::to_owned), self.worktrees.clone());
        let title = new.title.map(str::to_owned);
        let (pane, folder, kept) = on_core(move || {
            let title = title.as_deref();
            let (pane, folder) = match branch {
                Some(branch) => {
                    core.open_worktree_chat(agent, &launcher, &cwd, &branch, title, root.as_deref())
                }
                // Sheprd keeps a workspace for it, in this folder.
                None => core
                    .open_chat(
                        agent,
                        &launcher,
                        &cwd,
                        Some(core::Place::New { cwd: &cwd, title }),
                    )
                    .map(|pane| (pane, cwd)),
            }?;
            // Kept in the core's shared document at once, titled as the
            // phone shows it; its label still asks a desktop to keep it.
            let kept = core
                .share_new_workspace(&folder, &pane, title.unwrap_or(agent.display_name()))
                .unwrap_or_else(|error| {
                    tracing::warn!(%error, "A phone's new workspace waits for Sheprd to keep it");
                    None
                });
            Ok((pane, folder, kept))
        })
        .await?;
        // Otherwise listed apart until Sheprd keeps its workspace.
        Ok(CreatedWorkspace {
            workspace: kept.unwrap_or_else(|| core::new_workspace_id(&folder)),
            pane,
        })
    }

    /// A worktree Herdr 1 makes from `worktree.repository`'s workspace, in a
    /// workspace of its own there, with the chat on `core` while one
    /// answers, else typed into the worktree's first pane.
    async fn herdr_1_worktree(
        &self,
        core: Option<std::sync::Arc<core::Source>>,
        worktree: Worktree<'_>,
        agent: Agent,
        launcher: &Launcher,
    ) -> Result<CreatedWorkspace> {
        let created = self
            .call(ApiMethod::WorktreeCreate, worktree.params())
            .await?;
        let workspace = answered(&created["workspace"]["workspace_id"])?;
        let Some(core) = core else {
            let pane = answered(&created["root_pane"]["pane_id"])?;
            // The worktree's first pane is a shell in its folder; the program
            // path rides in this line because the pane was not made with it.
            self.run_line(&pane, &direct_command_line(launcher, agent))
                .await?;
            return Ok(CreatedWorkspace { workspace, pane });
        };
        let checkout = answered(&created["worktree"]["path"])?;
        let (labelled, launcher) = (workspace.clone(), launcher.clone());
        // Labelled for the new workspace, so Sheprd puts it in a tab there.
        let started = on_core(move || {
            core.open_chat(
                agent,
                &launcher,
                &checkout,
                Some(place(&labelled, &checkout)),
            )
        })
        .await;
        let pane = started.inspect_err(|error| {
            tracing::warn!(%error, %workspace, "A new worktree's chat did not start; the worktree stays");
        })?;
        Ok(CreatedWorkspace { workspace, pane })
    }

    /// `new` in Herdr 1, which no core stands in for: a workspace in its
    /// folder, and a worktree from it when a branch is given.
    async fn herdr_1_workspace(
        &self,
        new: NewWorkspace<'_>,
        launcher: &Launcher,
    ) -> Result<CreatedWorkspace> {
        let mut params = json!({"cwd": new.cwd, "focus": false});
        if let (Some(title), None) = (new.title, new.branch) {
            params["label"] = title.into();
        }
        let opened = self
            .call_in(new.session, ApiMethod::WorkspaceCreate, params)
            .await?;
        let workspace = answered(&opened["workspace"]["workspace_id"])?;
        let Some(branch) = new.branch else {
            let response = self
                .call(
                    ApiMethod::TabCreate,
                    tab_params(&workspace, new.cwd, new.agent, &launcher.program),
                )
                .await?;
            let (_, pane) = created_tab(&response).ok_or(Error::Response)?;
            self.run_line(&pane, &command_line(launcher, new.agent))
                .await?;
            return Ok(CreatedWorkspace { workspace, pane });
        };
        let worktree = Worktree {
            base: None,
            branch,
            repository: &workspace,
            title: new.title,
        };
        self.herdr_1_worktree(None, worktree, new.agent, launcher)
            .await
    }
}

/// A new worktree Herdr 1 makes.
#[derive(Clone, Copy, Debug)]
struct Worktree<'a> {
    /// The full ref it starts from; `None` for the repository's `HEAD`.
    base: Option<&'a str>,
    branch: &'a str,
    /// The workspace of the repository it is made from.
    repository: &'a str,
    title: Option<&'a str>,
}

impl Worktree<'_> {
    /// Its `worktree.create` parameters; a title names the new workspace.
    fn params(self) -> Value {
        let mut params = json!({
            "workspace_id": self.repository,
            "branch": self.branch,
            "focus": false,
        });
        if let Some(title) = self.title {
            params["label"] = title.into();
        }
        if let Some(base) = self.base {
            params["base"] = base.into();
        }
        params
    }
}

/// A non-empty id or path Herdr answered.
fn answered(value: &Value) -> Result<String> {
    value
        .as_str()
        .filter(|text| !text.is_empty())
        .map(str::to_owned)
        .ok_or(Error::Response)
}

/// Where Herdr 1 makes a new worktree from: the workspace of the
/// repository's own checkout, and the ref it starts from when that is not
/// the checkout's `HEAD`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Source {
    pub(super) workspace: String,
    pub(super) base: Option<String>,
}

/// The checkout a folder is in, as Git and Herdr 1 name it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Checkout {
    /// Its repository's Git directory, which Herdr 1 keys workspaces by.
    pub(super) common_dir: PathBuf,
    /// A linked worktree, not the repository's own checkout.
    pub(super) linked: bool,
    /// Its branch's full ref; `None` when detached.
    pub(super) branch: Option<String>,
}

impl Checkout {
    /// The checkout `cwd` is in, `None` outside one. Runs Git: call it
    /// off the async workers.
    fn of(cwd: &Path) -> Option<Self> {
        let dirs = git(
            cwd,
            &[
                "rev-parse",
                "--path-format=absolute",
                "--git-common-dir",
                "--git-dir",
            ],
        )
        .ok()?;
        let branch = git(cwd, &["symbolic-ref", "--quiet", "HEAD"]).ok();
        Self::parse(&dirs, branch.as_deref(), |path| {
            std::fs::canonicalize(path).unwrap_or_else(|_| path.to_owned())
        })
    }

    /// From `rev-parse`'s common and Git directories and `symbolic-ref`'s
    /// branch, with paths made canonical as Herdr 1's keys are.
    fn parse(
        dirs: &str,
        branch: Option<&str>,
        canonical: impl Fn(&Path) -> PathBuf,
    ) -> Option<Self> {
        let mut lines = dirs.lines().map(str::trim).filter(|line| !line.is_empty());
        let common_dir = canonical(Path::new(lines.next()?));
        let git_dir = canonical(Path::new(lines.next()?));
        let branch = branch
            .map(str::trim)
            .filter(|branch| branch.starts_with("refs/heads/"))
            .map(str::to_owned);
        Some(Self {
            linked: git_dir != common_dir,
            common_dir,
            branch,
        })
    }
}

/// Where Herdr 1 makes a new worktree in `checkout` from, among
/// `session`'s workspaces (the primary's when `None`): the workspace of
/// its repository's own checkout, keyed by its Git directory, and for a
/// linked worktree its branch, as the desktop's new worktree does: Herdr 1
/// refuses a linked checkout as the source.
///
/// `None` when Herdr 1 shows no such checkout, and the core makes it as
/// before. Workspaces Sheprd keeps for Herdr 2, a core's new workspaces,
/// and WSL's are not Herdr 1's to grow here.
pub(super) fn herdr_1_repository(
    layout: &Layout,
    checkout: &Checkout,
    session: Option<&str>,
    kept: impl Fn(&str) -> bool,
) -> Option<Source> {
    let base = match (checkout.linked, &checkout.branch) {
        (false, _) => None,
        (true, Some(branch)) => Some(branch.clone()),
        (true, None) => return None,
    };
    let workspace = layout.workspaces.iter().find(|workspace| {
        sessions::split_id(&workspace.id).0 == session
            && !sessions::in_wsl(&workspace.id)
            && !core::is_new_workspace(&workspace.id)
            && !kept(&workspace.id)
            && workspace.worktree.as_ref().is_some_and(|tree| {
                !tree.is_linked_worktree && Path::new(&tree.key) == checkout.common_dir
            })
    })?;
    Some(Source {
        workspace: workspace.id.clone(),
        base,
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests;
