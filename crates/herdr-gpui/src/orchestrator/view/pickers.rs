//! Project and Author, beside the backend pills under a section's search:
//! each a button opening a menu of checks, most items first, with a field
//! that narrows it, and each pick a chip that drops it. A pill per project
//! and per person grew a row for every dozen of them and pushed the list
//! down; the buttons stay one line however many there are.

use super::{IssueSource, OrchestratorView, rows::Tab};
use crate::{
    config::corners,
    i18n::fl,
    search_input::{self, SearchInput},
};
use gpui::{prelude::*, *};

const MENU_WIDTH: f32 = 280.;
/// Taller lists scroll.
const LIST_HEIGHT: f32 = 320.;
/// The most rows a menu draws; typing narrows to the rest.
const MENU_ROWS: usize = 200;

/// What a picker narrows the list by.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Facet {
    Project,
    Author,
}

impl Facet {
    fn name(self) -> &'static str {
        match self {
            Self::Project => "project",
            Self::Author => "author",
        }
    }

    fn label(self) -> String {
        match self {
            Self::Project => fl!("orchestrator-pickers-project"),
            Self::Author => fl!("orchestrator-pickers-author"),
        }
    }
}

/// The open picker menu and the field narrowing it.
pub(super) struct Picker {
    open: Option<Facet>,
    /// The open menu's button was pressed, so the click ending the press
    /// leaves it shut rather than opening it again.
    closing: Option<Facet>,
    filter: Entity<SearchInput>,
    query: String,
    _subscription: Subscription,
}

impl Picker {
    pub(super) fn new(look: &super::Look, cx: &mut Context<OrchestratorView>) -> Self {
        let filter = cx.new(|cx| {
            let mut input = SearchInput::new(cx);
            input.set_placeholder_with(|| fl!("orchestrator-pickers-narrow"), cx);
            input.set_frameless(cx);
            input.set_appearance(look.ui.clone(), look.theme.clone(), cx);
            input
        });
        let subscription = cx.subscribe(&filter, |this, filter, _: &search_input::Changed, cx| {
            this.picker.query = filter.read(cx).text().to_owned();
            cx.notify();
        });
        Self {
            open: None,
            closing: None,
            filter,
            query: String::new(),
            _subscription: subscription,
        }
    }
}

/// One check in a picker menu.
struct Choice {
    /// The repository key or login the pick stores.
    key: String,
    label: String,
    /// The debug selector's suffix: a project's place in the list, which
    /// stays put as the menu narrows, or a login.
    id: String,
    count: usize,
    on: bool,
}

impl OrchestratorView {
    /// The pickers this list offers: Project while several projects are
    /// listed (Linear's issues belong to none), Author on the Pull requests
    /// list while there are several people, or one picked.
    pub(super) fn offered_facets(&self) -> Vec<Facet> {
        let mut offered = Vec::new();
        if self.issue_source != IssueSource::Linear && self.projects_named().nth(1).is_some() {
            offered.push(Facet::Project);
        }
        if self.tab == Tab::PullRequests && (!self.authors.is_empty() || self.people().len() > 1) {
            offered.push(Facet::Author);
        }
        offered
    }

    /// Each choice `facet` offers, with how many of the shown list's items
    /// it holds: projects in their listed order, people most items first.
    fn choices(&self, facet: Facet) -> Vec<Choice> {
        match facet {
            Facet::Project => self
                .projects_named()
                .enumerate()
                .map(|(index, repo)| {
                    let rows = match self.tab {
                        Tab::Issues => &repo.issue_rows,
                        Tab::PullRequests => &repo.pull_request_rows,
                    };
                    Choice {
                        on: self.projects.contains(&repo.key),
                        key: repo.key.clone(),
                        label: self.project_label(repo),
                        id: index.to_string(),
                        count: rows.iter().filter(|row| row.depth == 0).count(),
                    }
                })
                .collect(),
            Facet::Author => {
                let mut people = self.people();
                // Stable, so people with as many stay by name.
                people.sort_by(|(_, left), (_, right)| right.cmp(left));
                people
                    .into_iter()
                    .map(|(login, count)| Choice {
                        on: self.authors.contains(&login),
                        id: login.clone(),
                        label: login.clone(),
                        key: login,
                        count,
                    })
                    .collect()
            }
        }
    }

    fn picks_mut(&mut self, facet: Facet) -> &mut std::collections::HashSet<String> {
        match facet {
            Facet::Project => &mut self.projects,
            Facet::Author => &mut self.authors,
        }
    }

    fn toggle_pick(&mut self, facet: Facet, key: String, cx: &mut Context<Self>) {
        let picks = self.picks_mut(facet);
        if !picks.remove(&key) {
            picks.insert(key);
        }
        self.refresh_rows();
        cx.notify();
    }

    fn clear_picks(&mut self, facet: Facet, cx: &mut Context<Self>) {
        self.picks_mut(facet).clear();
        self.refresh_rows();
        cx.notify();
    }

    /// Opens `facet`'s menu with an empty field that has the keyboard.
    fn open_picker(&mut self, facet: Facet, window: &mut Window, cx: &mut Context<Self>) {
        self.picker.open = Some(facet);
        self.sort_open = false;
        self.hide_open = false;
        self.picker.filter.update(cx, |filter, cx| filter.clear(cx));
        window.focus(&self.picker.filter.read(cx).focus.clone(), cx);
        cx.notify();
    }

    fn close_picker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.picker.open.take().is_some() {
            window.focus(&self.focus, cx);
        }
        cx.notify();
    }

    /// Keys while a picker menu is open: Escape shuts it and Enter checks
    /// the first choice left; the field keeps every other key from the list.
    /// True when the key was the picker's.
    pub(super) fn picker_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        // A menu left open on the other list is not drawn, so takes no keys.
        let Some(facet) = self
            .picker
            .open
            .filter(|facet| self.offered_facets().contains(facet))
        else {
            return false;
        };
        match event.keystroke.key.as_str() {
            "escape" => self.close_picker(window, cx),
            "enter" => {
                if let Some(choice) = self.narrowed(facet).into_iter().next() {
                    self.toggle_pick(facet, choice.key, cx);
                }
            }
            _ => return self.picker.filter.read(cx).focus.is_focused(window),
        }
        cx.stop_propagation();
        true
    }

    /// `facet`'s choices whose label holds what the field says.
    fn narrowed(&self, facet: Facet) -> Vec<Choice> {
        let query = self.picker.query.trim().to_lowercase();
        let mut choices = self.choices(facet);
        choices.retain(|choice| choice.label.to_lowercase().contains(&query));
        choices
    }

    /// Each offered picker's button, its menu under it while open, then a
    /// chip per pick.
    pub(super) fn render_pickers(
        &self,
        offered: &[Facet],
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let mut elements: Vec<AnyElement> = offered
            .iter()
            .map(|&facet| self.render_picker(facet, window, cx).into_any_element())
            .collect();
        for &facet in offered {
            for choice in self.choices(facet).into_iter().filter(|choice| choice.on) {
                elements.push(self.render_pick_chip(facet, choice, cx).into_any_element());
            }
        }
        elements
    }

    fn render_picker(&self, facet: Facet, window: &Window, cx: &mut Context<Self>) -> Div {
        let look = &self.look;
        let theme = &look.theme;
        let open = self.picker.open == Some(facet);
        let picked = self
            .choices(facet)
            .iter()
            .filter(|choice| choice.on)
            .count();
        let id = format!("orchestrator-picker-{}", facet.name());
        let selector = id.clone();
        let button = div()
            .id(SharedString::from(id))
            .debug_selector(move || selector)
            .role(Role::Button)
            .aria_label(facet.label())
            .flex_none()
            .flex()
            .items_center()
            .gap_1()
            .px_2()
            .py(px(2.))
            .rounded(px(corners::CONTROL))
            .border_1()
            .border_color(rgb(if open || picked > 0 {
                theme.primary()
            } else {
                theme.active
            }))
            .when(open, |el| el.bg(rgb(theme.primary_wash())))
            .cursor_pointer()
            .text_size(look.small())
            .child(look.icon(
                match facet {
                    Facet::Project => "icons/git-branch.svg",
                    Facet::Author => "icons/user.svg",
                },
                11.,
                theme.subtext(),
            ))
            .child(facet.label())
            .when(picked > 0, |el| {
                el.child(
                    div()
                        .px(px(5.))
                        .rounded_full()
                        .bg(rgb(theme.primary()))
                        .text_color(rgb(theme.text_on(theme.primary())))
                        .text_size(look.tiny())
                        .child(picked.to_string()),
                )
            })
            .child(look.icon("icons/chevron-down.svg", 10., theme.muted))
            // As in the settings dropdowns: a press on the open menu's
            // button shuts it, and its click must not open it again.
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _, window, cx| {
                    if open {
                        this.picker.closing = Some(facet);
                        this.close_picker(window, cx);
                    } else {
                        this.picker.closing = None;
                    }
                }),
            )
            .on_click(cx.listener(move |this, _, window, cx| {
                if this.picker.closing.take() != Some(facet) {
                    this.open_picker(facet, window, cx);
                }
            }));
        div().relative().flex_none().child(button).when(open, |el| {
            // Hung from the button's bottom left corner.
            el.child(
                div().absolute().top_full().left_0().child(
                    deferred(anchored().anchor(Anchor::TopLeft).snap_to_window().child(
                        crate::motion::pop_in(
                            self.render_picker_menu(facet, window, cx),
                            "orchestrator-picker-menu-in",
                        ),
                    ))
                    .with_priority(1),
                ),
            )
        })
    }

    /// The field, Clear while something is picked, and a check per choice
    /// left after narrowing, up to [`MENU_ROWS`].
    fn render_picker_menu(
        &self,
        facet: Facet,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let look = &self.look;
        let theme = &look.theme;
        let hover = theme.active;
        let picked = self.choices(facet).iter().any(|choice| choice.on);
        let mut choices = self.narrowed(facet);
        let more = choices.len().saturating_sub(MENU_ROWS);
        choices.truncate(MENU_ROWS);
        let none = choices.is_empty();
        let focused = self.picker.filter.read(cx).focus.is_focused(window);
        let row = |id: String| {
            let selector = id.clone();
            div()
                .id(SharedString::from(id))
                .debug_selector(move || selector)
                .flex_none()
                .flex()
                .items_center()
                .gap_2()
                .px_2()
                .h(px(28.))
                .rounded(px(corners::SMALL))
                .cursor_pointer()
                .hover(move |style| style.bg(rgb(hover)))
        };
        let rows = choices.into_iter().map(|choice| {
            let on = choice.on;
            let key = choice.key;
            row(format!("orchestrator-pick-{}-{}", facet.name(), choice.id))
                .child(
                    div()
                        .flex_none()
                        .size(px(11.))
                        .rounded(px(2.))
                        .border_1()
                        .border_color(rgb(if on { theme.primary() } else { theme.muted }))
                        .when(on, |el| el.bg(rgb(theme.primary()))),
                )
                .child(div().flex_1().min_w_0().truncate().child(choice.label))
                .when(choice.count > 0, |el| {
                    el.child(look.muted(choice.count.to_string()))
                })
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.toggle_pick(facet, key.clone(), cx);
                }))
        });
        div()
            .id("orchestrator-picker-menu")
            .debug_selector(|| "orchestrator-picker-menu".into())
            .mt_1()
            .w(px(MENU_WIDTH))
            .p_1()
            .flex()
            .flex_col()
            .gap_1()
            .rounded(px(corners::CONTROL))
            .bg(rgb(theme.surface))
            .border_1()
            .border_color(rgb(theme.active))
            .shadow_lg()
            .occlude()
            .text_size(look.small())
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_mouse_down_out(cx.listener(|this, _, window, cx| {
                this.close_picker(window, cx);
            }))
            .child(
                look.search_frame("orchestrator-picker-filter", focused)
                    .flex_none()
                    .child(div().flex_1().min_w_0().child(self.picker.filter.clone())),
            )
            .when(picked, |el| {
                el.child(
                    row(format!("orchestrator-pick-{}-clear", facet.name()))
                        .text_color(rgb(theme.subtext()))
                        .child(look.icon("icons/close.svg", 9., theme.muted))
                        .child(fl!("orchestrator-pickers-clear"))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.clear_picks(facet, cx);
                        })),
                )
            })
            .child(
                div()
                    .id("orchestrator-picker-list")
                    .max_h(px(LIST_HEIGHT))
                    .overflow_y_scroll()
                    .flex()
                    .flex_col()
                    .when(none, |el| {
                        el.child(
                            look.muted(fl!("orchestrator-pickers-no-match"))
                                .px_2()
                                .py_1(),
                        )
                    })
                    .children(rows)
                    .when(more > 0, |el| {
                        el.child(
                            look.muted(fl!("orchestrator-pickers-more", count = more))
                                .px_2()
                                .py_1(),
                        )
                    }),
            )
    }

    /// A pick, outlined in the accent, that drops it when clicked.
    fn render_pick_chip(
        &self,
        facet: Facet,
        choice: Choice,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let look = &self.look;
        let key = choice.key;
        look.pill(
            format!("orchestrator-picked-{}-{}", facet.name(), choice.id),
            true,
        )
        .child(choice.label)
        .child(look.icon("icons/close.svg", 9., look.theme.muted))
        .on_click(cx.listener(move |this, _, _, cx| {
            this.toggle_pick(facet, key.clone(), cx);
        }))
    }
}
