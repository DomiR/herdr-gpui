//! A section's list: its column heads and virtualized lines, headings
//! between rows of issues or pull requests, each row a click to open. While
//! an item is open beside the list, rows keep only the columns that fit.

use super::{
    IssueSource, OrchestratorView,
    look::{BLUE, CYAN, GREEN, Look, MAGENTA, RED, YELLOW, age, agent_icon},
    pr_cells::{checks_count, checks_dot, review_dot, review_text, row_tone, standing_word},
    rows::{Heading, ItemRow, Line, Next, Tab},
    tasks::status_label,
};
use crate::{
    fonts::StyledFont,
    i18n::fl,
    orchestrator::{Access, Item, Provider, SyncState},
    pull_request::tone_color,
};
use gpui::{prelude::*, *};
use std::{cell::Cell, rc::Rc};

/// Row heights at the usual text size; [`Look::row`] grows them with it.
/// A list's headings take its rows' height, as the list is uniform.
const ISSUE_ROW: f32 = 32.;
const PULL_REQUEST_ROW: f32 = 40.;
/// The Linear section's rows, as roomy as Linear's own list.
const LINEAR_ROW: f32 = 40.;
/// The pull request column: its mark and a number of up to five digits,
/// which ran into the next column at 48.
const PULL_REQUEST_WIDTH: f32 = 60.;
/// The project column, while several projects are listed.
const PROJECT_WIDTH: f32 = 130.;
/// The source column, while the list mixes backends.
const SOURCE_WIDTH: f32 = 22.;
/// The column holding a row's action, such as Start agent.
const ACTION_WIDTH: f32 = 96.;
/// The status column of the Tasks list.
const TASK_STATUS_WIDTH: f32 = 78.;
/// The author column of each list.
const ISSUE_AUTHOR_WIDTH: f32 = 110.;
const PULL_REQUEST_AUTHOR_WIDTH: f32 = 100.;
/// What a title keeps before an author joins it beside an open item.
pub(super) const AUTHOR_TITLE_MIN: f32 = 280.;
/// The gap between a row's cells, and its padding on either side.
const CELL_GAP: f32 = 8.;
const ROW_PADDING: f32 = 16.;
/// The list width below which the search field takes its own line, at the
/// usual text size: the Pull requests list's controls fill the row.
const SEARCH_BESIDE_WIDTH: f32 = 960.;
/// What that line adds above a dropdown.
const SEARCH_LINE: f32 = 40.;
/// The columns saying where a pull request stands, at the usual text size:
/// see [`Look::column`].
const STATUS_WIDTH: f32 = 80.;
const CHECKS_WIDTH: f32 = 70.;
/// The review column, wide enough for "approved with suggestions".
const REVIEW_WIDTH: f32 = 168.;

/// Why the list is empty.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Empty {
    NoRepositories,
    Loading,
    /// The search matches nothing.
    Search,
    /// The pills or the Issues facets hide everything.
    Filtered,
    Nothing,
}
impl OrchestratorView {
    /// Whether rows keep only the columns that fit beside an open item.
    pub(super) fn compact(&self) -> bool {
        self.detail.is_some()
    }

    /// Whether the search field sits above its controls, not beside them.
    pub(super) fn search_stacked(&self) -> bool {
        self.tab == Tab::PullRequests && self.list_width.get() < self.search_fit()
    }

    fn search_fit(&self) -> f32 {
        self.look.column(SEARCH_BESIDE_WIDTH)
    }

    /// How far a dropdown moves down for the search field's own line.
    pub(super) fn search_line(&self) -> f32 {
        if self.search_stacked() {
            SEARCH_LINE
        } else {
            0.
        }
    }

    /// Whether rows name their author: always, unless an open item leaves
    /// the title too little room for it.
    fn shows_author(&self) -> bool {
        self.tab != Tab::Tasks && (!self.compact() || self.list_width.get() >= self.author_fit())
    }

    /// The list width at which a row beside an open item fits its author.
    pub(super) fn author_fit(&self) -> f32 {
        let (cells, fixed) = match self.tab {
            // Age, run, title, author, and action.
            Tab::Issues | Tab::Tasks => (5., 32. + 30. + ISSUE_AUTHOR_WIDTH + ACTION_WIDTH),
            // Number, title, author, two dots, and age.
            Tab::PullRequests => (6., 48. + PULL_REQUEST_AUTHOR_WIDTH + 12. + 12. + 32.),
        };
        let (source_cells, source) = if self.mixed_sources {
            (1., SOURCE_WIDTH)
        } else {
            (0., 0.)
        };
        let (project_cells, project) = if self.listing_projects() {
            (1., PROJECT_WIDTH)
        } else {
            (0., 0.)
        };
        let gaps = cells + source_cells + project_cells - 1.;
        fixed + source + project + AUTHOR_TITLE_MIN + gaps * CELL_GAP + 2. * ROW_PADDING
    }

    /// A row's author, in a column `width` wide.
    fn author_cell(&self, item: &Item, width: f32) -> Div {
        let look = &self.look;
        div()
            .debug_selector(|| "orchestrator-row-author".into())
            .flex_none()
            .w(px(width))
            .truncate()
            .text_size(look.small())
            .text_color(rgb(look.theme.subtext()))
            .child(item.author.clone().unwrap_or_default())
    }

    /// Whether rows of several projects are listed, so each names its own.
    pub(super) fn listing_projects(&self) -> bool {
        // Linear's issues are the user's, not a project's.
        self.issue_source != IssueSource::Linear
            && self.projects_listing().count() > 1
            && self.projects.len() != 1
    }

    /// A row's project, in its own column while several are listed: its
    /// checkout's name, or the repository's and how many hosts list it when
    /// several checkouts of it do. A long name is cut, never the host count.
    fn project_cell(&self, row: &ItemRow) -> Option<Div> {
        if !self.listing_projects() {
            return None;
        }
        let look = &self.look;
        let repo = &self.repos[row.repo];
        let item = self.row_item(row);
        let (name, hosts) = if matches!(
            repo.remote,
            Some(crate::orchestrator::RemoteScope::Mine { .. })
        ) {
            // An organization's list mixes repositories: each row names its own.
            let source = &item.key.source.repository;
            (source.rsplit('/').next().unwrap_or(source).to_owned(), None)
        } else {
            match self.twins(&item.key.canonical()) {
                // Projects sharing a Beads database are different projects,
                // not one on several hosts.
                _ if item.key.source.provider == Provider::Beads => {
                    (self.project_label(repo), None)
                }
                0 => (self.project_label(repo), None),
                twins => (
                    repo.name(),
                    Some(fl!("orchestrator-project-hosts", count = twins + 1)),
                ),
            }
        };
        Some(
            div()
                .flex_none()
                .w(px(PROJECT_WIDTH))
                .flex()
                .items_center()
                .gap_1()
                .text_size(look.small())
                .text_color(rgb(look.theme.subtext()))
                .child(div().min_w_0().truncate().child(name))
                .children(hosts.map(|hosts| {
                    div()
                        .flex_none()
                        .whitespace_nowrap()
                        .text_color(rgb(look.theme.muted))
                        .child(hosts)
                })),
        )
    }

    /// A row's source as a muted mark, while the list mixes backends.
    fn source_cell(&self, provider: Provider) -> Option<Div> {
        self.mixed_sources.then(|| {
            div()
                .flex_none()
                .w(px(SOURCE_WIDTH))
                .child(source_mark(&self.look, provider))
        })
    }

    /// Whether the list is the Linear section's issues, which it draws as
    /// Linear's own list (`linear_rows.rs`).
    fn linear_list(&self) -> bool {
        self.tab == Tab::Issues && self.issue_source == IssueSource::Linear
    }

    pub(super) fn row_height(&self) -> f32 {
        self.look.row(match self.tab {
            Tab::Issues if self.linear_list() => LINEAR_ROW,
            Tab::Issues | Tab::Tasks => ISSUE_ROW,
            Tab::PullRequests => PULL_REQUEST_ROW,
        })
    }

    pub(super) fn render_list(&self, cx: &mut Context<Self>) -> AnyElement {
        let look = &self.look;
        let compact = self.compact();
        let author = self.shows_author();
        let sources = self.mixed_sources;
        let projects = self.listing_projects();
        let mut head: Vec<(String, Option<f32>)> = Vec::new();
        match self.tab {
            Tab::Issues | Tab::Tasks => {
                head.push((fl!("orchestrator-column-age"), Some(32.)));
                if sources {
                    head.push((fl!("orchestrator-column-source"), Some(SOURCE_WIDTH)));
                }
                head.push((fl!("orchestrator-column-run"), Some(30.)));
                if projects {
                    head.push((fl!("orchestrator-column-project"), Some(PROJECT_WIDTH)));
                }
                head.push((fl!("orchestrator-column-title"), None));
                if !compact && self.tab == Tab::Tasks {
                    head.extend([
                        (fl!("orchestrator-column-assignee"), Some(110.)),
                        (fl!("orchestrator-column-status"), Some(TASK_STATUS_WIDTH)),
                    ]);
                }
                if author {
                    head.push((fl!("orchestrator-column-author"), Some(ISSUE_AUTHOR_WIDTH)));
                }
                if !compact && self.tab != Tab::Tasks {
                    head.push(("\u{1f4ac}".into(), Some(34.)));
                }
                head.push((String::new(), Some(ACTION_WIDTH)));
            }
            Tab::PullRequests => {
                head.push((
                    fl!("orchestrator-column-pull-request"),
                    Some(PULL_REQUEST_WIDTH),
                ));
                if sources {
                    head.push((fl!("orchestrator-column-source"), Some(SOURCE_WIDTH)));
                }
                if projects {
                    head.push((fl!("orchestrator-column-project"), Some(PROJECT_WIDTH)));
                }
                head.push((fl!("orchestrator-column-title-branch"), None));
                if author {
                    head.push((
                        fl!("orchestrator-column-author"),
                        Some(PULL_REQUEST_AUTHOR_WIDTH),
                    ));
                }
                if compact {
                    head.extend([(String::new(), Some(12.)), (String::new(), Some(12.))]);
                } else {
                    head.extend([
                        (
                            fl!("orchestrator-column-status"),
                            Some(look.column(STATUS_WIDTH)),
                        ),
                        (
                            fl!("orchestrator-column-review"),
                            Some(look.column(REVIEW_WIDTH)),
                        ),
                        (
                            fl!("orchestrator-column-checks"),
                            Some(look.column(CHECKS_WIDTH)),
                        ),
                        (fl!("orchestrator-column-size"), Some(70.)),
                        (fl!("orchestrator-column-run"), Some(30.)),
                    ]);
                }
                head.push((fl!("orchestrator-column-age"), Some(32.)));
            }
        }
        let count = self.lines.len();
        let list = if count == 0 {
            self.render_empty(cx).into_any_element()
        } else {
            uniform_list(
                "orchestrator-rows",
                count,
                cx.processor(|this, range: std::ops::Range<usize>, _, cx| {
                    range.map(|index| this.render_line(index, cx)).collect()
                }),
            )
            .track_scroll(&self.scroll)
            .flex_1()
            .min_h_0()
            .into_any_element()
        };
        div()
            .debug_selector(|| "orchestrator-list".into())
            .relative()
            .flex_1()
            .min_w_0()
            .overflow_hidden()
            .flex()
            .flex_col()
            .child(measure(
                self.list_width.clone(),
                [self.author_fit(), self.search_fit()],
            ))
            // Column heads over nothing to line up are clutter, and Linear's
            // rows carry no columns.
            .when(count > 0 && !self.linear_list(), |el| {
                el.child(column_head(look, &head))
            })
            .child(list)
            .into_any_element()
    }

    /// Why nothing is listed, which picks what the empty list says.
    pub(super) fn empty(&self) -> Empty {
        let loading = self.repos.iter().any(|repo| {
            let snapshot = &repo.snapshot;
            snapshot.error.is_none()
                && (snapshot.repo.is_none()
                    || snapshot
                        .sources
                        .iter()
                        .any(|status| matches!(status.state, SyncState::Syncing { .. })))
        });
        let searching = !self.query.trim().is_empty();
        // The pills narrow both tabs; each tab also has its own filters.
        let filtered = !self.projects.is_empty()
            || (self.tab != Tab::Tasks && !self.backends.is_empty())
            || match self.tab {
                Tab::Issues | Tab::Tasks => self.active_filters() > 0 || self.hiding(),
                Tab::PullRequests => {
                    self.filters.mine
                        || self.filters.review_requested
                        || self.filters.has_run
                        || !self.authors.is_empty()
                }
            };
        if self.repos.is_empty() {
            Empty::NoRepositories
        } else if loading && !searching {
            Empty::Loading
        } else if searching {
            Empty::Search
        } else if filtered {
            Empty::Filtered
        } else {
            Empty::Nothing
        }
    }

    fn render_empty(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let look = &self.look;
        let issues = self.tab == Tab::Issues;
        let tasks = self.tab == Tab::Tasks;
        let query = self.query.trim();
        // With nothing listed at all, the section says what it is for.
        let (icon, text, about) = match self.empty() {
            Empty::NoRepositories => (
                "icons/git-branch.svg",
                fl!("orchestrator-no-repositories"),
                Some(fl!("orchestrator-no-repositories-about")),
            ),
            Empty::Loading => ("icons/refresh.svg", fl!("orchestrator-loading"), None),
            Empty::Search if tasks => (
                "icons/search.svg",
                fl!("orchestrator-no-tasks-match", query = query),
                None,
            ),
            Empty::Search if issues => (
                "icons/search.svg",
                fl!("orchestrator-no-issues-match", query = query),
                None,
            ),
            Empty::Search => (
                "icons/search.svg",
                fl!("orchestrator-no-pull-requests-match", query = query),
                None,
            ),
            Empty::Filtered if tasks => (
                "icons/search.svg",
                fl!("orchestrator-no-tasks-filtered"),
                None,
            ),
            Empty::Filtered if issues => (
                "icons/search.svg",
                fl!("orchestrator-no-issues-filtered"),
                None,
            ),
            Empty::Filtered => (
                "icons/search.svg",
                fl!("orchestrator-no-pull-requests-filtered"),
                None,
            ),
            Empty::Nothing if tasks => (
                "icons/check.svg",
                fl!("orchestrator-no-tasks"),
                Some(fl!("orchestrator-no-tasks-about")),
            ),
            Empty::Nothing if issues => (
                "icons/check.svg",
                fl!("orchestrator-no-issues"),
                Some(fl!("orchestrator-no-issues-about")),
            ),
            Empty::Nothing => (
                "icons/check.svg",
                fl!("orchestrator-no-pull-requests"),
                Some(fl!("orchestrator-no-pull-requests-about")),
            ),
        };
        div()
            .debug_selector(|| "orchestrator-empty".into())
            .flex_1()
            .px_6()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap_2()
            .child(look.icon(icon, 22., look.theme.muted))
            .child(div().font_weight(FontWeight::SEMIBOLD).child(text))
            .children(about.map(|about| {
                div()
                    .max_w(px(460.))
                    .text_center()
                    .text_color(rgb(look.theme.muted))
                    .child(about)
            }))
            .when(!query.is_empty(), |el| {
                el.child(
                    look.button(
                        "orchestrator-clear",
                        fl!("orchestrator-clear-filter"),
                        false,
                    )
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.search.update(cx, |search, cx| search.clear(cx));
                    })),
                )
            })
    }

    fn render_line(&self, index: usize, cx: &mut Context<Self>) -> AnyElement {
        match self.lines.get(index) {
            Some(Line::Heading { heading, count }) => {
                self.render_heading(index, *heading, *count, cx)
            }
            Some(Line::Row(row)) => match self.tab {
                Tab::Issues if self.linear_list() => self.render_linear_row(index, *row, cx),
                Tab::Issues | Tab::Tasks => self.render_issue_row(index, *row, cx),
                Tab::PullRequests => self.render_pull_request_row(index, *row, cx),
            },
            None => div().h(px(self.row_height())).into_any_element(),
        }
    }

    fn render_heading(
        &self,
        index: usize,
        heading: Heading,
        count: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let look = &self.look;
        let theme = &look.theme;
        let (label, hue): (String, u32) = match heading {
            Heading::Assignee(group) => return self.render_assignee(index, group, count, cx),
            Heading::State(group) => return self.render_state(index, group, count, cx),
            Heading::Next(next) => match next {
                Next::Review => (fl!("orchestrator-next-review"), look.hue(MAGENTA)),
                Next::Fix => (fl!("orchestrator-next-fix"), look.hue(RED)),
                Next::Merge => (fl!("orchestrator-next-merge"), look.hue(GREEN)),
                Next::Wait => (fl!("orchestrator-next-wait"), theme.muted),
                Next::Closed => (fl!("orchestrator-next-closed"), theme.muted),
            },
            Heading::Priority(Some(priority)) => (
                priority_name(priority),
                look.hue(look.priority_hue(priority)),
            ),
            Heading::Priority(None) => (fl!("orchestrator-no-priority"), theme.muted),
            Heading::Creator(row) => match &self.row_item(&row).author {
                Some(author) => (author.clone(), look.hue(BLUE)),
                None => (fl!("orchestrator-linear-unknown-author"), theme.muted),
            },
        };
        div()
            .w_full()
            .h(px(self.row_height()))
            .flex()
            .items_center()
            .gap_2()
            .px_4()
            .bg(rgb(theme.surface))
            .border_b_1()
            .border_color(rgb(theme.active))
            .child(look.dot(hue, 7.))
            .child(
                div()
                    .text_size(look.small())
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(label),
            )
            .child(look.muted(count.to_string()))
            .into_any_element()
    }

    /// A row's shell: selection, and a click to open it.
    /// `item` names the row for a screen reader by its identifier, title, and
    /// state.
    pub(super) fn row_shell(
        &self,
        id: (&'static str, usize),
        item: &Item,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let key = item.key.canonical();
        let theme = &self.look.theme;
        let selected = self.selected.as_deref() == Some(key.as_str());
        let open = self.detail.as_ref().is_some_and(|detail| detail.key == key);
        let hover = theme.surface;
        div()
            .id(id)
            .role(Role::ListItem)
            .aria_label(fl!(
                "orchestrator-row-label",
                identifier = item.identifier.as_str(),
                title = item.title.as_str(),
                state = item.state.as_str()
            ))
            .aria_selected(selected)
            .w_full()
            .h(px(self.row_height()))
            .flex()
            .items_center()
            .gap_2()
            .px_4()
            .border_b_1()
            .border_color(rgb(theme.active))
            .cursor_pointer()
            .hover(move |style| style.bg(rgb(hover)))
            // The accent marks only the row whose page is open; a row the
            // keys moved to, with no page, wears the hover's quiet tint.
            .when(open, |el| el.bg(rgb(theme.primary_wash())))
            .when(selected && !open, |el| el.bg(rgb(hover)))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _: &MouseDownEvent, _, cx| {
                    this.selected = Some(key.clone());
                    this.open_detail(key.clone(), cx);
                }),
            )
    }

    fn render_issue_row(&self, index: usize, row: ItemRow, cx: &mut Context<Self>) -> AnyElement {
        let look = &self.look;
        let theme = &look.theme;
        let compact = self.compact();
        let item = self.row_item(&row);
        let key = item.key.canonical();
        let now = chrono::Utc::now();
        let expander = row.children.map(|open| {
            let key = key.clone();
            div()
                .id(("orchestrator-expand", index))
                .child(look.icon(
                    if open {
                        "icons/chevron-down.svg"
                    } else {
                        "icons/chevron-right.svg"
                    },
                    10.,
                    theme.muted,
                ))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        this.toggle_children(key.clone(), cx);
                    }),
                )
        });
        let blocked = blocked(item);
        let tasks = self.tab == Tab::Tasks;
        let start = self.start_button(index, &row, cx);
        let worktree = self.worktree_button(&row, super::worktree::ROW_GROUP, cx);
        self.row_shell(("orchestrator-issue", index), item, cx)
            .debug_selector(move || format!("orchestrator-row-{key}"))
            .group(super::worktree::ROW_GROUP)
            .child(
                div()
                    .flex_none()
                    .w(px(32.))
                    .text_size(look.small())
                    .text_color(rgb(theme.muted))
                    .child(age(item.updated_at, now)),
            )
            .children(self.source_cell(item.key.source.provider))
            .child(self.run_mark(&row))
            .children(self.project_cell(&row))
            .child(
                div()
                    .debug_selector(|| "orchestrator-row-title".into())
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .items_center()
                    .gap_2()
                    .pl(px(f32::from(row.depth) * 18.))
                    .child(div().flex_none().w(px(10.)).children(expander))
                    .child(
                        look.mono(item.identifier.clone())
                            .flex_none()
                            .text_color(rgb(theme.muted)),
                    )
                    .children(self.status_pill(item))
                    .child(div().min_w_0().truncate().child(item.title.clone()))
                    .when(!compact, |el| {
                        el.children(
                            item.labels
                                .iter()
                                .take(2)
                                .map(|label| look.chip(label.clone())),
                        )
                    })
                    .when(blocked, |el| {
                        el.child(look.icon("icons/lock.svg", 11., look.hue(YELLOW)))
                    }),
            )
            .when(!compact && tasks, |el| {
                el.child(
                    div()
                        .flex_none()
                        .w(px(110.))
                        .truncate()
                        .text_size(look.small())
                        .text_color(rgb(theme.subtext()))
                        .child(item.assignee.clone().unwrap_or_default()),
                )
                .child(
                    div()
                        .flex_none()
                        .w(px(TASK_STATUS_WIDTH))
                        .truncate()
                        .text_size(look.small())
                        .text_color(rgb(look.hue(look.state_hue(&item.state))))
                        .child(status_label(&item.state)),
                )
            })
            .when(self.shows_author(), |el| {
                el.child(self.author_cell(item, ISSUE_AUTHOR_WIDTH))
            })
            .when(!compact && !tasks, |el| {
                el.child(
                    div()
                        .flex_none()
                        .w(px(34.))
                        .text_size(look.small())
                        .text_color(rgb(theme.muted))
                        .child(
                            item.activity
                                .and_then(|activity| activity.comments)
                                .filter(|count| *count > 0)
                                .map(|count| count.to_string())
                                .unwrap_or_default(),
                        ),
                )
            })
            .child(
                div()
                    .flex_none()
                    .w(px(ACTION_WIDTH))
                    .flex()
                    .justify_end()
                    .children(start)
                    .children(worktree)
                    .when(blocked, |el| {
                        el.child(look.badge(fl!("orchestrator-badge-blocked"), YELLOW))
                    }),
            )
            .into_any_element()
    }

    fn render_pull_request_row(
        &self,
        index: usize,
        row: ItemRow,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let look = &self.look;
        let theme = &look.theme;
        let compact = self.compact();
        let item = self.row_item(&row);
        let Some(pr) = &item.pull_request else {
            return div().h(px(self.row_height())).into_any_element();
        };
        let status = super::rows::status_of(item, &self.repos[row.repo].snapshot.pr_status);
        let fork = pr
            .head_repository
            .as_deref()
            .is_some_and(|head| head != item.key.source.repository);
        let draft = item.state == "draft";
        let tone = row_tone(item, status);
        let key = item.key.canonical();
        self.row_shell(("orchestrator-pr", index), item, cx)
            .debug_selector(move || format!("orchestrator-row-{key}"))
            .child(
                div()
                    .flex_none()
                    .w(px(PULL_REQUEST_WIDTH))
                    .flex()
                    .items_center()
                    .gap_1()
                    .child(look.icon("icons/pull-request.svg", 12., tone_color(tone, theme)))
                    .child(
                        look.mono(item.identifier.clone())
                            .text_color(rgb(theme.muted)),
                    ),
            )
            .children(self.source_cell(item.key.source.provider))
            .children(self.project_cell(&row))
            .child(
                div()
                    .debug_selector(|| "orchestrator-row-title".into())
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(div().min_w_0().truncate().child(item.title.clone()))
                            .when(draft, |el| {
                                el.child(look.badge(fl!("orchestrator-badge-draft"), BLUE))
                            }),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_1()
                            .text_size(look.tiny())
                            .text_color(rgb(theme.muted))
                            .child(look.icon("icons/git-branch.svg", 10., theme.muted))
                            .child(
                                div()
                                    .min_w_0()
                                    .truncate()
                                    .text_font(&look.mono)
                                    .child(pr.head_ref.clone()),
                            )
                            .when(fork, |el| {
                                el.child(look.badge(fl!("orchestrator-badge-fork"), CYAN))
                            }),
                    ),
            )
            .when(self.shows_author(), |el| {
                el.child(self.author_cell(item, PULL_REQUEST_AUTHOR_WIDTH))
            })
            .map(|el| {
                if compact {
                    el.child(
                        div()
                            .flex_none()
                            .w(px(12.))
                            .children(review_dot(look, status)),
                    )
                    .child(
                        div()
                            .flex_none()
                            .w(px(12.))
                            .children(checks_dot(look, status)),
                    )
                } else {
                    el.child(
                        div()
                            .flex_none()
                            .w(px(look.column(STATUS_WIDTH)))
                            .overflow_hidden()
                            .flex()
                            .children(
                                standing_word(tone, item, status)
                                    .map(|word| look.badge_in(word, tone_color(tone, theme))),
                            ),
                    )
                    .child(
                        div()
                            .debug_selector({
                                let key = item.key.canonical();
                                move || format!("orchestrator-review-{key}")
                            })
                            .flex_none()
                            .w(px(look.column(REVIEW_WIDTH)))
                            .flex()
                            .overflow_hidden()
                            .children(
                                review_text(status, self.repos[row.repo].reading_login())
                                    .map(|(word, hue)| look.badge(word, hue)),
                            ),
                    )
                    .child(
                        div()
                            .flex_none()
                            .w(px(look.column(CHECKS_WIDTH)))
                            .overflow_hidden()
                            .flex()
                            .children(checks_count(look, status)),
                    )
                    .child(diff_squares(look, pr.additions, pr.deletions).w(px(70.)))
                    .child(self.run_mark(&row))
                }
            })
            .child(
                div()
                    .flex_none()
                    .w(px(32.))
                    .text_size(look.small())
                    .text_color(rgb(theme.muted))
                    .child(age(item.updated_at, chrono::Utc::now())),
            )
            .into_any_element()
    }

    /// Start agent on `row`, when its repository is writable and its item
    /// neither blocked nor already run.
    pub(super) fn start_button(
        &self,
        index: usize,
        row: &ItemRow,
        cx: &mut Context<Self>,
    ) -> Option<Stateful<Div>> {
        let item = self.row_item(row);
        let writable = self.repos[row.repo].snapshot.access == Some(Access::ReadWrite);
        if !writable || row.run.is_some() || blocked(item) || super::rows::closed(item) {
            return None;
        }
        let key = item.key.canonical();
        Some(
            self.look
                .compact_button(
                    ("orchestrator-start", index),
                    fl!("orchestrator-start-agent"),
                )
                .debug_selector(|| "orchestrator-start".into())
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.open_dispatch(key.clone(), window, cx);
                })),
        )
    }

    pub(super) fn run_mark(&self, row: &ItemRow) -> Div {
        let look = &self.look;
        div()
            .flex_none()
            .w(px(30.))
            .flex()
            .items_center()
            .gap_1()
            .when_some(row.run, |el, run| {
                let agent = &self.repos[row.repo].snapshot.runs[run.run].agent;
                el.child(look.icon(agent_icon(agent), 13., look.theme.foreground))
                    .child(look.dot(look.status_hue(run.status), 7.))
            })
    }
}

/// Whether `item` waits on another, or its tracker says it is blocked.
pub(super) fn blocked(item: &Item) -> bool {
    !item.blocked_by.is_empty() || item.state == "blocked"
}

/// Records the list's width as it lays out, and draws again when that moves
/// it across a width in `fits`, so rows gain or lose their author and the
/// search field its line on the next frame.
fn measure(width: Rc<Cell<f32>>, fits: [f32; 2]) -> impl IntoElement {
    canvas(
        move |bounds, window, _| {
            let now = f32::from(bounds.size.width);
            let before = width.replace(now);
            if fits.iter().any(|fit| (before >= *fit) != (now >= *fit)) {
                window.refresh();
            }
        },
        |_, _, _, _| {},
    )
    .absolute()
    .inset_0()
}

/// A backend's muted mark in a row.
pub(super) fn source_mark(look: &Look, provider: Provider) -> AnyElement {
    look.icon(source_icon(provider), 12., look.theme.muted)
        .into_any_element()
}

pub(super) fn source_icon(provider: Provider) -> &'static str {
    match provider {
        Provider::Azure => "icons/azure-devops.svg",
        Provider::Github => "icons/github.svg",
        Provider::Beads => "icons/beads.svg",
        Provider::Gitlab => "icons/gitlab.svg",
        Provider::Linear => "icons/linear.svg",
    }
}

/// A button small enough to sit in a row with room above and below.
fn column_head(look: &Look, cells: &[(String, Option<f32>)]) -> Div {
    let theme = &look.theme;
    div()
        .flex_none()
        .flex()
        .items_center()
        .gap_2()
        .h(px(24.))
        .px_4()
        .border_b_1()
        .border_color(rgb(theme.active))
        .text_size(look.tiny())
        .text_color(rgb(theme.muted))
        .children(cells.iter().map(|(text, width)| {
            match width {
                Some(width) => div()
                    .flex_none()
                    .w(px(*width))
                    .whitespace_nowrap()
                    .child(SharedString::from(text.clone())),
                None => div().flex_1().child(SharedString::from(text.clone())),
            }
        }))
}

/// Five squares, green for added lines and red for removed, in proportion.
pub(super) fn diff_squares(look: &Look, additions: Option<u64>, deletions: Option<u64>) -> Div {
    let (added, removed) = (additions.unwrap_or(0), deletions.unwrap_or(0));
    let total = added + removed;
    let green = (added * 5 + total / 2)
        .checked_div(total)
        .map_or(0, |green| green.min(5));
    let known = additions.is_some() || deletions.is_some();
    div()
        .flex_none()
        .w(px(44.))
        .flex()
        .items_center()
        .gap(px(1.))
        .children((0..5).map(move |index| {
            let color = if !known {
                look.theme.active
            } else if index < green {
                look.hue(GREEN)
            } else {
                look.hue(RED)
            };
            div().size(px(7.)).rounded(px(1.)).bg(rgb(color))
        }))
}

/// A priority's heading: `P1 · high`.
pub(super) fn priority_name(priority: i64) -> String {
    fl!("orchestrator-priority", priority = priority)
}
