//! The Linear section's new issue dialog, like Linear's own: a team, a
//! title, a Markdown description, and a row of chips that each open a small
//! picker (status, priority, assignee, project, estimate, labels, cycle).
//! New issue in the header or C opens it, Cmd-Enter creates, Escape closes.
//!
//! What the pickers offer is read by the worker that lists Linear, which
//! alone holds the API key, the first time the dialog opens; the worker
//! creates the issue too, and the snapshot carries both answers back. The
//! dialog applies them when it is next drawn ([`OrchestratorView::settle_create`]).

mod menu;
mod parent;
mod render;

use super::{IssueSource, OrchestratorView};
use crate::i18n::fl;
use crate::{
    orchestrator::{
        Notice,
        linear::{NewIssue, Options, TeamOptions},
    },
    search_input::SearchInput,
    text_area::{TextArea, TextAreaEvent},
};
use gpui::{prelude::*, *};
use menu::Menu;
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

/// How long after a creation its page opens once the listing shows it;
/// an issue outside the listed scopes never shows.
const OPEN_WITHIN: Duration = Duration::from_secs(30);

/// The dialog, while open, and what outlives it.
#[derive(Default)]
pub(super) struct Create {
    pub(super) dialog: Option<Dialog>,
    /// Creations asked for so far, which numbers the next.
    sequence: u64,
    /// The issue just created, by identifier, whose page opens once the
    /// listing shows it, and when it was created.
    open: Option<(String, Instant)>,
}

impl Create {
    /// Closes the dialog; whether one was open.
    pub(super) fn close(&mut self) -> bool {
        self.dialog.take().is_some()
    }
}

/// An issue being written.
pub(super) struct Dialog {
    pub(super) title: Entity<SearchInput>,
    pub(super) description: Entity<TextArea>,
    /// The team's id; `None` until the choices arrive.
    pub(super) team: Option<String>,
    /// The workflow state's id.
    pub(super) state: Option<String>,
    /// Linear's: 0 none, 1 urgent to 4 low.
    pub(super) priority: i64,
    pub(super) assignee: Option<String>,
    pub(super) project: Option<String>,
    pub(super) estimate: Option<i64>,
    /// Label ids, in the order picked.
    pub(super) labels: Vec<String>,
    pub(super) cycle: Option<String>,
    /// The state the dialog was opened for, such as a lane's, applied once
    /// the choices arrive.
    asked_state: Option<String>,
    /// Create more: after creating, the dialog stays open for the next.
    pub(super) more: bool,
    /// The issue the new one is a sub-issue of.
    pub(super) parent: Option<parent::Parent>,
    pub(super) menu: Option<Menu>,
    /// Why the last attempt did not create the issue.
    pub(super) problem: Option<SharedString>,
    /// The issue the dialog last created while Create more was on.
    pub(super) created: Option<String>,
    /// The sequence of the creation under way.
    pub(super) sending: Option<u64>,
    _events: Subscription,
}

impl Dialog {
    /// Sets what a new issue in `team` starts with: its default state, or
    /// the one asked for when the team has it; the key owner when a member;
    /// the active cycle. What the team does not offer is cleared.
    fn start_in(&mut self, team: &TeamOptions, viewer: &str) {
        self.team = Some(team.id.clone());
        let asked = self.asked_state.as_deref().and_then(|id| team.state(id));
        self.state = asked
            .or_else(|| team.default_state())
            .and_then(|state| state.id.clone());
        let member = |id: &str| team.members.iter().any(|member| member.id == id);
        self.assignee = match self.assignee.take() {
            Some(id) if member(&id) => Some(id),
            _ => member(viewer).then(|| viewer.to_owned()),
        };
        self.project = None;
        self.cycle = team.active_cycle.clone();
        let scale = team.estimates.as_deref().unwrap_or_default();
        self.estimate = self
            .estimate
            .filter(|value| scale.iter().any(|point| point.value == *value));
        self.labels
            .retain(|id| team.labels.iter().any(|label| label.id == *id));
    }
}

impl OrchestratorView {
    /// Opens the new issue dialog, its title taking the keyboard; with
    /// `status`, a workflow state's id, the issue starts in that state when
    /// its team has it, as a lane's or group's own button asks. An open
    /// dialog keeps what was written.
    pub(crate) fn open_linear_create(
        &mut self,
        status: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.create.dialog.is_none() {
            let dialog = self.new_dialog(window, cx);
            self.create.dialog = Some(dialog);
        }
        if let Some(dialog) = &mut self.create.dialog {
            if status.is_some() {
                dialog.asked_state = status;
                dialog.team = None;
            }
            let focus = dialog.title.read(cx).focus.clone();
            window.focus(&focus, cx);
        }
        // Choices that failed to read are read again.
        let read = matches!(self.linear_options(), None | Some(Err(_)));
        if read && let Some(service) = self.linear_repo().and_then(|repo| repo.service.as_ref()) {
            service.read_linear_options();
        }
        self.apply_options();
        cx.notify();
    }

    fn new_dialog(&self, window: &mut Window, cx: &mut Context<Self>) -> Dialog {
        let title = cx.new(|cx| {
            let mut input = SearchInput::new(cx);
            input.set_placeholder_with(|| fl!("orchestrator-linear-issue-title"), cx);
            input.set_frameless(cx);
            let mut font = self.look.ui.clone();
            font.size += 6.;
            input.set_appearance(font, self.look.theme.clone(), cx);
            input
        });
        let (description, events) = self.description_area(window, cx);
        Dialog {
            title,
            description,
            team: None,
            state: None,
            priority: 0,
            assignee: None,
            project: None,
            estimate: None,
            labels: Vec::new(),
            cycle: None,
            asked_state: None,
            more: false,
            parent: None,
            menu: None,
            problem: None,
            created: None,
            sending: None,
            _events: events,
        }
    }

    /// An empty description field: Cmd-Enter in it creates the issue and
    /// Escape closes the picker or the dialog.
    fn description_area(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> (Entity<TextArea>, Subscription) {
        let area = cx.new(|cx| {
            let mut area = TextArea::new(cx);
            area.set_placeholder(&fl!("orchestrator-linear-add-description"), cx);
            area.set_appearance(self.look.ui.clone(), self.look.theme.clone(), cx);
            area
        });
        let events = cx.subscribe_in(&area, window, |this, _, event, window, cx| match event {
            TextAreaEvent::Submit => this.submit_create(cx),
            TextAreaEvent::Cancel => this.escape_create(window, cx),
            TextAreaEvent::Changed => {}
        });
        (area, events)
    }

    /// What the worker that lists Linear read for the dialog, if anything.
    fn linear_options(&self) -> Option<&Result<Arc<Options>, Arc<crate::orchestrator::Error>>> {
        self.linear_repo()?.snapshot.linear_create.options.as_ref()
    }

    /// The choices, once read.
    fn options(&self) -> Option<Arc<Options>> {
        self.linear_options()?.as_ref().ok().cloned()
    }

    /// Starts the dialog in a team once the choices arrive: the one the
    /// asked state belongs to, else the first.
    fn apply_options(&mut self) {
        let Some(options) = self.options() else {
            return;
        };
        let Some(dialog) = self
            .create
            .dialog
            .as_mut()
            .filter(|dialog| dialog.team.is_none())
        else {
            return;
        };
        let asked = dialog.asked_state.as_deref();
        // A sub-issue starts in its parent's team.
        let parent_team = dialog.parent.as_ref().map(parent::Parent::team_key);
        let team = options
            .teams
            .iter()
            .find(|team| parent_team == Some(team.key.as_str()))
            .or_else(|| {
                options
                    .teams
                    .iter()
                    .find(|team| asked.is_some_and(|id| team.state(id).is_some()))
            })
            .or_else(|| options.teams.first());
        if let Some(team) = team {
            dialog.start_in(team, &options.viewer);
        }
    }

    /// Moves the dialog to the team `id`, starting it over there.
    fn pick_team(&mut self, id: &str) {
        let Some(options) = self.options() else {
            return;
        };
        let (Some(dialog), Some(team)) = (
            self.create.dialog.as_mut(),
            options.teams.iter().find(|team| team.id == id),
        ) else {
            return;
        };
        if dialog.team.as_deref() != Some(id) {
            dialog.asked_state = None;
            dialog.start_in(team, &options.viewer);
        }
    }

    /// The team the dialog is in, once the choices arrive.
    fn create_team(&self) -> Option<(Arc<Options>, usize)> {
        let options = self.options()?;
        let id = self.create.dialog.as_ref()?.team.as_deref()?;
        let index = options.teams.iter().position(|team| team.id == id)?;
        Some((options, index))
    }

    /// Asks the worker that lists Linear to create the dialog's issue.
    pub(super) fn submit_create(&mut self, cx: &mut Context<Self>) {
        let sequence = self.create.sequence + 1;
        let service = self
            .repos
            .iter()
            .find(|repo| repo.request.linear.is_some())
            .and_then(|repo| repo.service.as_ref());
        let Some(dialog) = &mut self.create.dialog else {
            return;
        };
        if dialog.sending.is_some() {
            return;
        }
        dialog.menu = None;
        let title = dialog.title.read(cx).text().trim().to_owned();
        let problem = match (&dialog.team, title.is_empty()) {
            (None, _) => Some(fl!("orchestrator-linear-wait-for-teams")),
            (_, true) => Some(fl!("orchestrator-linear-give-title")),
            _ => None,
        };
        if let Some(problem) = problem {
            dialog.problem = Some(problem.into());
            cx.notify();
            return;
        }
        let issue = NewIssue {
            team: dialog.team.clone().unwrap_or_default(),
            title,
            description: dialog.description.read(cx).text().trim().to_owned(),
            state: dialog.state.clone(),
            priority: dialog.priority,
            assignee: dialog.assignee.clone(),
            project: dialog.project.clone(),
            estimate: dialog.estimate,
            labels: dialog.labels.clone(),
            cycle: dialog.cycle.clone(),
            parent: dialog.parent.as_ref().map(|parent| parent.id.clone()),
            sequence,
        };
        if service.is_some_and(|service| service.create_linear_issue(issue)) {
            self.create.sequence = sequence;
            dialog.sending = Some(sequence);
            dialog.problem = None;
        } else {
            dialog.problem = Some(fl!("orchestrator-linear-busy").into());
        }
        cx.notify();
    }

    /// Escape: closes the open picker, else the dialog.
    fn escape_create(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        match &mut self.create.dialog {
            Some(dialog) if dialog.menu.is_some() => {
                dialog.menu = None;
                cx.notify();
            }
            _ => {
                self.close_overlays(window, cx);
            }
        }
    }

    /// Keys while the dialog is open, which no other part of the view
    /// sees: Escape closes, Cmd-Enter creates, Enter in the title goes on to
    /// the description. Whether the key was the dialog's.
    pub(super) fn create_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(dialog) = &self.create.dialog else {
            return false;
        };
        if dialog.title.read(cx).is_composing() {
            return false;
        }
        let modifiers = &event.keystroke.modifiers;
        match event.keystroke.key.as_str() {
            "escape" => self.escape_create(window, cx),
            "enter" if modifiers.platform || modifiers.control => self.submit_create(cx),
            "enter" if dialog.title.read(cx).focus.is_focused(window) => {
                let focus = dialog.description.read(cx).focus.clone();
                window.focus(&focus, cx);
            }
            _ => return false,
        }
        true
    }

    /// Applies what the worker answered since the dialog was last drawn:
    /// the choices, and the creation under way. Created, the dialog closes,
    /// or with Create more starts the next issue, and the new issue's page
    /// opens once the listing shows it.
    pub(super) fn settle_create(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.open_created(cx);
        self.apply_options();
        let Some(sending) = self
            .create
            .dialog
            .as_ref()
            .and_then(|dialog| dialog.sending)
        else {
            return;
        };
        let Some(outcome) = self
            .linear_repo()
            .and_then(|repo| repo.snapshot.linear_create.created.as_ref())
            .filter(|outcome| outcome.sequence >= sending)
            .cloned()
        else {
            return;
        };
        let Some(dialog) = &mut self.create.dialog else {
            return;
        };
        dialog.sending = None;
        let created = match outcome.result {
            Ok(created) => created,
            Err(error) => {
                dialog.problem = Some(error.to_string().into());
                cx.notify();
                return;
            }
        };
        if dialog.more {
            dialog.title.update(cx, SearchInput::clear);
            let (description, events) = self.description_area(window, cx);
            let Some(dialog) = &mut self.create.dialog else {
                return;
            };
            dialog.description = description;
            dialog._events = events;
            dialog.created = Some(created.identifier);
            let focus = dialog.title.read(cx).focus.clone();
            window.focus(&focus, cx);
        } else {
            self.create.dialog = None;
            self.create.open = Some((created.identifier, Instant::now()));
            self.notice = Some(Notice {
                outcome: Ok(herdr_orchestrator::Done::LinearIssueCreated),
            });
            window.focus(&self.focus, cx);
        }
        cx.notify();
    }

    /// Opens the page of the issue just created once the listing shows it.
    fn open_created(&mut self, cx: &mut Context<Self>) {
        let Some((identifier, at)) = &self.create.open else {
            return;
        };
        if at.elapsed() > OPEN_WITHIN || self.issue_source != IssueSource::Linear {
            self.create.open = None;
            return;
        }
        let key = self
            .repos
            .iter()
            .flat_map(|repo| repo.snapshot.items.iter())
            .find(|item| {
                item.key.source.provider == crate::orchestrator::Provider::Linear
                    && item.identifier == *identifier
            })
            .map(|item| item.key.canonical());
        if let Some(key) = key {
            self.create.open = None;
            self.selected = Some(key.clone());
            self.open_detail(key, cx);
        }
    }

    /// Whether the dialog is open, for the view's keys.
    pub(super) fn creating(&self) -> bool {
        self.create.dialog.is_some()
    }
}

impl OrchestratorView {
    /// A "+" that opens the new issue dialog in `state`, for a lane's or a
    /// status group's head; named `name` and `index` for its selector.
    pub(super) fn new_issue_plus(
        &self,
        name: &'static str,
        index: usize,
        state: Option<String>,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let look = &self.look;
        div()
            .id((SharedString::from(format!("{name}-new-issue")), index))
            .debug_selector(move || format!("{name}-new-issue-{index}"))
            .flex_none()
            .px_1()
            .rounded(px(crate::config::corners::SMALL))
            .text_color(rgb(look.theme.muted))
            .cursor_pointer()
            .hover(|style| style.bg(rgb(look.theme.active)))
            .child("+")
            .on_click(cx.listener(move |this, _, window, cx| {
                cx.stop_propagation();
                this.open_linear_create(state.clone(), window, cx);
            }))
    }
}
