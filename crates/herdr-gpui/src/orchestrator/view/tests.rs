use super::rows::*;
use crate::orchestrator::{
    Activity, Backend, HerdrSession, Item, ItemKey, Owner, Provider, Run, RunState, SourceKey,
    Workspace,
};
use chrono::{DateTime, Utc};
use herdr_client::protocol::AgentStatus;

mod banners;
mod clicks;
mod conversation;
mod conversation_list;
mod dialog;
mod empty;
mod failure_text;
mod gitlab_merge_request;
mod groups;
mod linear;
mod linear_board;
mod linear_cards;
mod linear_create;
mod linear_dialog;
mod linear_drag;
mod linear_facets;
mod linear_filters;
mod linear_hidden;
mod linear_layout;
mod linear_list;
mod linear_reorder;
mod linear_repos;
mod linear_scroll;
mod linear_shown;
mod linear_sub_issues;
mod list_switch;
mod markdown;
mod non_projects;
mod outside_git;
mod pills;
mod pr_creators;
mod pr_empty;
mod refresh;
mod row_start;
mod rows;
mod section_banners;
mod sessions;
mod sign_in;
mod source_chip;
mod status;
mod twin_text;
mod twins;

fn at(value: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(value)
        .unwrap()
        .with_timezone(&Utc)
}

fn source(provider: Provider) -> SourceKey {
    SourceKey {
        provider,
        host: if provider == Provider::Beads {
            "local"
        } else {
            "github.com"
        }
        .into(),
        repository: "penso/app".into(),
    }
}

fn item(provider: Provider, id: &str, title: &str, updated: &str) -> Item {
    Item {
        key: ItemKey {
            source: source(provider),
            native_id: id.into(),
        },
        identifier: id.into(),
        title: title.into(),
        description: None,
        state: "open".into(),
        url: None,
        author: Some("penso".into()),
        labels: Vec::new(),
        parent_id: None,
        blocked_by: Vec::new(),
        priority: None,
        created_at: Some(at(updated)),
        updated_at: Some(at(updated)),
        pull_request: None,
        activity: Some(Activity::default()),
        tracker: None,
    }
}

fn run(id: &str, item: &Item, state: RunState, started: &str) -> Run {
    Run {
        id: id.into(),
        item_key: item.key.canonical(),
        workspace: Some(Workspace {
            backend: Backend::Herdr,
            id: format!("w-{id}"),
            host: None,
            path: None,
            branch: format!("b-{id}"),
        }),
        agent: "claude".into(),
        model: None,
        state,
        message: None,
        session_id: None,
        started_at: at(started),
        updated_at: at(started),
        owner: Owner::HerdrGpui,
    }
}

fn session(run: &Run, pane: &str) -> HerdrSession {
    HerdrSession {
        run_id: run.id.clone(),
        host: None,
        session: None,
        workspace_id: format!("w-{}", run.id),
        pane_id: pane.into(),
        agent_name: format!("herdr-gpui-{}", run.id),
        updated_at: run.started_at,
    }
}

fn live(run: &Run, pane: &str, status: AgentStatus) -> LiveAgent {
    LiveAgent {
        endpoint: 0,
        host: None,
        session: None,
        workspace_id: format!("w-{}", run.id),
        pane_id: pane.into(),
        status,
        title: Some("Running tests".into()),
    }
}

/// Draws the view inside it, until told to stop as leaving the section
/// does: GPUI then drops the view's elements' own state.
pub(super) struct Host {
    pub(super) view: gpui::Entity<super::OrchestratorView>,
    pub(super) shown: bool,
}

impl gpui::Render for Host {
    fn render(
        &mut self,
        _: &mut gpui::Window,
        _: &mut gpui::Context<Self>,
    ) -> impl gpui::IntoElement {
        use gpui::{ParentElement, Styled, prelude::FluentBuilder};
        gpui::div()
            .size_full()
            .flex()
            .when(self.shown, |el| el.child(self.view.clone()))
    }
}

/// A view of one repository listing `items` on the Linear section's
/// issues, in a `width` wide window, with a worker that takes every move
/// and answers none, so a test says when each is done.
pub(super) fn open_linear(
    cx: &mut gpui::TestAppContext,
    items: Vec<Item>,
    width: f32,
) -> (
    gpui::Entity<Host>,
    gpui::Entity<super::OrchestratorView>,
    &mut gpui::VisualTestContext,
) {
    use crate::orchestrator::{IssueSource, Request, Service, Timing};
    use gpui::AppContext;
    let dir = tempfile::tempdir().unwrap();
    let request = Request {
        target: herdr_client::ConnectTarget::Local,
        checkout: dir.path().join("missing").to_string_lossy().into_owned(),
        workspace_id: "w1".into(),
        token: None,
        data_root: Some(dir.path().join("data")),
        gitlab_store: Default::default(),
        timing: Timing::default(),
        linear: None,
    };
    let config = crate::config::Config::default();
    let look = super::Look {
        theme: crate::config::Theme::default(),
        ui: config.ui.clone(),
        mono: config.terminal.clone(),
    };
    let (host, cx) = cx.add_window_view(|_, cx| Host {
        view: cx.new(|cx| super::OrchestratorView::with_repo(request, look, cx)),
        shown: true,
    });
    cx.simulate_resize(gpui::size(gpui::px(width), gpui::px(600.)));
    let view = cx.update(|_, cx| host.read(cx).view.clone());
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.repo_mut().service = Some(Service::unserved());
            view.repo_mut().snapshot.items = std::sync::Arc::new(items);
            view.tab = Tab::Issues;
            view.set_issue_source(IssueSource::Linear, cx);
            view.refresh_rows();
            cx.notify();
        })
    });
    cx.run_until_parked();
    (host, view, cx)
}

/// Clicks the element `selector` names.
pub(super) fn click_on(cx: &mut gpui::VisualTestContext, selector: &str) {
    // Test selectors are few; leaking them gives the `'static` lookups need.
    let selector: &'static str = selector.to_owned().leak();
    let at = cx
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("{selector}"));
    cx.simulate_click(at.center(), gpui::Modifiers::none());
    cx.run_until_parked();
}
