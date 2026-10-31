//! A section's list: its column heads and virtualized lines, headings
//! between rows of issues or pull requests, each row a click to open. While
//! an item is open beside the list, rows keep only the columns that fit.

use super::{
    IssueSource, OrchestratorView,
    look::{BLUE, CYAN, GREEN, Look, MAGENTA, RED, YELLOW, age, agent_icon},
    rows::{Heading, ItemRow, Line, Next, Tab},
};
use crate::{
    fonts::StyledFont,
    orchestrator::{Access, CheckState, PrStatus, Provider, Review, SyncState},
};
use gpui::{prelude::*, *};

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
/// The checks column, which counts checks by state.
const CHECKS_WIDTH: f32 = 96.;
/// The source column, while the list mixes backends.
const SOURCE_WIDTH: f32 = 22.;
/// The column holding a row's action, such as Start agent.
const ACTION_WIDTH: f32 = 96.;

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

    /// Whether rows of several projects are listed, so each names its own.
    fn listing_projects(&self) -> bool {
        // Linear's issues are the user's, not a project's.
        self.issue_source != IssueSource::Linear
            && self.projects_named().count() > 1
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
        let (name, hosts) = match self.twins(&self.row_item(row).key.canonical()) {
            0 => (self.project_label(repo), None),
            twins => (repo.name(), Some(format!("{} hosts", twins + 1))),
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
            Tab::Issues => ISSUE_ROW,
            Tab::PullRequests => PULL_REQUEST_ROW,
        })
    }

    pub(super) fn render_list(&self, cx: &mut Context<Self>) -> AnyElement {
        let look = &self.look;
        let compact = self.compact();
        let sources = self.mixed_sources;
        let projects = self.listing_projects();
        let mut head: Vec<(&str, Option<f32>)> = Vec::new();
        match self.tab {
            Tab::Issues => {
                head.push(("AGE", Some(32.)));
                if sources {
                    head.push(("SRC", Some(SOURCE_WIDTH)));
                }
                head.push(("RUN", Some(30.)));
                if projects {
                    head.push(("PROJECT", Some(PROJECT_WIDTH)));
                }
                head.push(("TITLE", None));
                if !compact {
                    head.extend([("AUTHOR", Some(110.)), ("\u{1f4ac}", Some(34.))]);
                }
                head.push(("", Some(ACTION_WIDTH)));
            }
            Tab::PullRequests => {
                head.push(("PR", Some(PULL_REQUEST_WIDTH)));
                if sources {
                    head.push(("SRC", Some(SOURCE_WIDTH)));
                }
                if projects {
                    head.push(("PROJECT", Some(PROJECT_WIDTH)));
                }
                head.push(("TITLE \u{00b7} BRANCH", None));
                if compact {
                    head.extend([("", Some(12.)), ("", Some(12.))]);
                } else {
                    head.extend([
                        ("AUTHOR", Some(100.)),
                        ("REVIEW", Some(124.)),
                        ("CHECKS", Some(CHECKS_WIDTH)),
                        ("SIZE", Some(70.)),
                        ("RUN", Some(30.)),
                    ]);
                }
                head.push(("AGE", Some(32.)));
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
            .flex_1()
            .min_w_0()
            .flex()
            .flex_col()
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
            || !self.backends.is_empty()
            || match self.tab {
                Tab::Issues => self.active_filters() > 0 || self.hiding(),
                Tab::PullRequests => {
                    self.filters.mine || self.filters.has_run || !self.authors.is_empty()
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
        let what = match self.tab {
            Tab::Issues => "open issues",
            Tab::PullRequests => "pull requests",
        };
        let query = self.query.trim();
        // With nothing listed at all, the section says what it is for.
        let (icon, text, about) = match self.empty() {
            Empty::NoRepositories => (
                "icons/git-branch.svg",
                "No repositories".to_owned(),
                Some(
                    "Pull requests and issues come from the Git checkouts your workspaces \
                     are in, on this machine or an SSH host. Open a workspace in one to list \
                     them here.",
                ),
            ),
            Empty::Loading => ("icons/refresh.svg", "Loading\u{2026}".to_owned(), None),
            Empty::Search => (
                "icons/search.svg",
                format!("No {what} match \u{201c}{query}\u{201d}"),
                None,
            ),
            Empty::Filtered => (
                "icons/search.svg",
                format!("No {what} match the filters"),
                None,
            ),
            Empty::Nothing => {
                let about = match self.tab {
                    Tab::Issues => {
                        "This lists the open issues of every repository your workspaces \
                         hold, from GitHub, GitLab, and Beads. Pick one to read it or to \
                         start an agent on it."
                    }
                    Tab::PullRequests => {
                        "This lists the pull requests of every repository your workspaces \
                         hold, with their reviews and checks. Pick one to read it or to \
                         start an agent on it."
                    }
                };
                ("icons/check.svg", format!("No {what}"), Some(about))
            }
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
                    look.button("orchestrator-clear", "Clear filter", false)
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
                Tab::Issues => self.render_issue_row(index, *row, cx),
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
                Next::Review => ("Review requested".into(), look.hue(MAGENTA)),
                Next::Fix => ("Fix".into(), look.hue(RED)),
                Next::Merge => ("Ready to merge".into(), look.hue(GREEN)),
                Next::Wait => ("Waiting".into(), theme.muted),
                Next::Closed => ("Closed and merged".into(), theme.muted),
            },
            Heading::Priority(Some(priority)) => (
                format!("P{priority} \u{00b7} {}", priority_name(priority)),
                look.hue(look.priority_hue(priority)),
            ),
            Heading::Priority(None) => ("No priority".into(), theme.muted),
            Heading::Creator(row) => match &self.row_item(&row).author {
                Some(author) => (author.clone(), look.hue(BLUE)),
                None => ("Unknown author".into(), theme.muted),
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
        item: &crate::orchestrator::model::Item,
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
            .aria_label(format!(
                "{} {}, {}",
                item.identifier, item.title, item.state
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
            .when(!compact, |el| {
                el.child(
                    div()
                        .flex_none()
                        .w(px(110.))
                        .truncate()
                        .text_size(look.small())
                        .text_color(rgb(theme.subtext()))
                        .child(item.author.clone().unwrap_or_default()),
                )
                .child(
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
                    .when(blocked, |el| el.child(look.badge("blocked", YELLOW))),
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
                    .child(look.icon(
                        "icons/pull-request.svg",
                        12.,
                        if draft { theme.muted } else { look.hue(GREEN) },
                    ))
                    .child(
                        look.mono(item.identifier.clone())
                            .text_color(rgb(theme.muted)),
                    ),
            )
            .children(self.source_cell(item.key.source.provider))
            .children(self.project_cell(&row))
            .child(
                div()
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
                            .when(draft, |el| el.child(look.badge("draft", BLUE))),
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
                            .when(fork, |el| el.child(look.badge("fork", CYAN))),
                    ),
            )
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
                            .w(px(100.))
                            .truncate()
                            .text_size(look.small())
                            .text_color(rgb(theme.subtext()))
                            .child(item.author.clone().unwrap_or_default()),
                    )
                    .child(div().flex_none().w(px(124.)).flex().children(review_badge(
                        look,
                        status,
                        self.repos[row.repo].reading_login(),
                    )))
                    .child(
                        div()
                            .flex_none()
                            .w(px(CHECKS_WIDTH))
                            .overflow_hidden()
                            .flex()
                            .items_center()
                            .gap_2()
                            .children(check_counts(look, status)),
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
        if !writable || row.run.is_some() || blocked(item) {
            return None;
        }
        let key = item.key.canonical();
        Some(
            self.look
                .compact_button(("orchestrator-start", index), "Start agent")
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
pub(super) fn blocked(item: &crate::orchestrator::Item) -> bool {
    !item.blocked_by.is_empty() || item.state == "blocked"
}

/// A backend's muted mark in a row.
pub(super) fn source_mark(look: &Look, provider: Provider) -> AnyElement {
    look.icon(source_icon(provider), 12., look.theme.muted)
        .into_any_element()
}

pub(super) fn source_icon(provider: Provider) -> &'static str {
    match provider {
        Provider::Github => "icons/github.svg",
        Provider::Beads => "icons/beads.svg",
        Provider::Gitlab => "icons/gitlab.svg",
        Provider::Linear => "icons/linear.svg",
    }
}

/// A button small enough to sit in a row with room above and below.
fn column_head(look: &Look, cells: &[(&str, Option<f32>)]) -> Div {
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
                    .child(SharedString::from(text.to_string())),
                None => div().flex_1().child(SharedString::from(text.to_string())),
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

fn priority_name(priority: i64) -> &'static str {
    match priority {
        0 => "urgent",
        1 => "high",
        2 => "normal",
        3 => "low",
        _ => "backlog",
    }
}

fn check_hue(look: &Look, state: CheckState) -> u32 {
    match state {
        CheckState::Failed => look.hue(RED),
        CheckState::Pending => look.hue(YELLOW),
        CheckState::Passed => look.hue(GREEN),
        CheckState::Skipped => look.theme.muted,
    }
}

/// The review state's word, or that the user is asked to review.
fn review_badge(look: &Look, status: Option<&PrStatus>, login: Option<&str>) -> Option<Div> {
    let status = status?;
    if login.is_some_and(|login| status.requests(login)) {
        return Some(look.badge("your review", MAGENTA));
    }
    Some(match status.review? {
        Review::Approved => look.badge("approved", GREEN),
        Review::ChangesRequested => look.badge("changes requested", RED),
        Review::Required => look.badge("review required", BLUE),
    })
}

fn review_dot(look: &Look, status: Option<&PrStatus>) -> Option<Div> {
    let hue = match status?.review? {
        Review::Approved => GREEN,
        Review::ChangesRequested => RED,
        Review::Required => BLUE,
    };
    Some(look.dot(look.hue(hue), 7.))
}

fn checks_dot(look: &Look, status: Option<&PrStatus>) -> Option<Div> {
    let state = status?.checks_state()?;
    Some(look.dot(check_hue(look, state), 7.))
}

/// How many of `status`'s checks failed, are pending, and passed, in that
/// order, leaving out states none are in. Skipped checks are not counted.
pub(super) fn check_tally(status: &PrStatus) -> Vec<(CheckState, usize)> {
    [CheckState::Failed, CheckState::Pending, CheckState::Passed]
        .into_iter()
        .map(|state| {
            let count = status
                .checks
                .iter()
                .filter(|check| check.state == state)
                .count();
            (state, count)
        })
        .filter(|(_, count)| *count > 0)
        .collect()
}

/// How many checks failed, are pending, and passed, failures first, each a
/// mark and a count in its state's color. Names cut to fit a column read as
/// noise ("poli labe gene"), and a pull request can run dozens of checks;
/// the item's page lists them by name.
fn check_counts(look: &Look, status: Option<&PrStatus>) -> Vec<Div> {
    let Some(status) = status else {
        return Vec::new();
    };
    let mut counts = Vec::new();
    for (state, count) in check_tally(status) {
        let hue = check_hue(look, state);
        let mark = match state {
            CheckState::Failed => look.icon("icons/close.svg", 9., hue),
            CheckState::Passed => look.icon("icons/check.svg", 11., hue),
            CheckState::Pending | CheckState::Skipped => look.dot(hue, 6.).into_any_element(),
        };
        counts.push(
            div()
                .flex_none()
                .flex()
                .items_center()
                .gap(px(3.))
                .text_size(look.small())
                .text_color(rgb(hue))
                .child(mark)
                .child(count.to_string()),
        );
    }
    counts
}
