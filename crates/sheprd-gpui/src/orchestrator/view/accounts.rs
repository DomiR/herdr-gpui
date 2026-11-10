//! The accounts the view's repositories are read with: GitHub's on each
//! host, the GitLab hosts and Azure DevOps organizations whose tokens it
//! needs, and syncing again when a saved credential changes.

use super::{GitLabRead, OrchestratorView};
use crate::orchestrator::{Provider, SyncState};
use gpui::Context;

impl OrchestratorView {
    /// The account GitHub is read as on `target`; a change syncs that host's
    /// repositories again at once and reads the open pull request again.
    pub(crate) fn set_github(
        &mut self,
        target: &sheprd_client::ConnectTarget,
        token: Option<crate::github::Tokens>,
        login: Option<String>,
        cx: &mut Context<Self>,
    ) {
        let mut changed = false;
        for repo in self
            .repos
            .iter_mut()
            .filter(|repo| repo.remote.is_none() && repo.request.target == *target)
        {
            repo.login = login.clone();
            let same = match (&token, &repo.request.token) {
                (Some(new), Some(old)) => new.same(old),
                (None, None) => true,
                _ => false,
            };
            if same {
                continue;
            }
            repo.request.token = token.clone();
            if let Some(service) = &repo.service {
                service.refresh(token.clone());
            }
            changed = true;
        }
        if !changed {
            return;
        }
        // What the old account read goes, and the open pull request is read
        // again as the new one: a lookup made signed out was dropped.
        self.pr.track(None);
        self.reload_pull_request();
        self.follow_pull_request();
        cx.notify();
    }

    /// The GitLab hosts this view's repositories are read from.
    pub(crate) fn gitlab_hosts(&self) -> impl Iterator<Item = GitLabRead> + '_ {
        self.repos.iter().flat_map(|repo| {
            let info = repo.snapshot.repo.as_ref();
            // A remote checkout's .envrc is that machine's, not run here.
            let checkout = (!repo.request.target.is_remote())
                .then(|| std::path::PathBuf::from(&repo.request.checkout));
            repo.snapshot
                .sources
                .iter()
                .filter(|status| status.key.provider == Provider::Gitlab)
                .map(move |status| GitLabRead {
                    host: status.key.host.clone(),
                    scheme: sheprd_orchestrator::gitlab::Scheme::of_source(info, &status.key),
                    checkout: checkout.clone(),
                })
        })
    }

    /// A `provider` source waits for a token that works.
    pub(crate) fn needs_sign_in(&self, provider: Provider) -> bool {
        self.repos
            .iter()
            .flat_map(|repo| &repo.snapshot.sources)
            .any(|status| {
                status.key.provider == provider && matches!(status.state, SyncState::SignIn)
            })
    }

    /// The Azure DevOps organizations this view's repositories are read
    /// from, lowercase, as tokens are saved for them.
    pub(crate) fn azure_organizations(&self) -> impl Iterator<Item = String> + '_ {
        self.repos
            .iter()
            .flat_map(|repo| &repo.snapshot.sources)
            .filter(|status| status.key.provider == Provider::Azure)
            .filter_map(|status| {
                sheprd_orchestrator::azure::Repository::organization(&status.key)
                    .map(str::to_ascii_lowercase)
            })
    }

    /// A saved credential changed: sync again, as Refresh does.
    pub(crate) fn credentials_changed(&mut self, cx: &mut Context<Self>) {
        self.refresh();
        cx.notify();
    }
}
