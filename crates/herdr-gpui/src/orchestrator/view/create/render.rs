//! Drawing the new issue dialog: the team chip and close button, the title
//! and description, the property chips with their pickers, and Create more
//! beside Create issue.

use super::{
    Dialog,
    menu::{Menu, PRIORITIES, priority_hue},
};
use crate::orchestrator::{
    IssueSource,
    linear::TeamOptions,
    view::{OrchestratorView, dispatch::card, look::RED, rows::Tab},
};
use gpui::{prelude::*, *};

/// As wide as Linear's own dialog, narrower in a narrow window.
const WIDTH: f32 = 680.;

/// What a chip reads and its dot, or `None` while unset, when it reads
/// its own name, muted.
type Shown = Option<(String, Option<u32>)>;

impl OrchestratorView {
    /// New issue, in the Linear section's header.
    pub(in crate::orchestrator::view) fn render_new_issue(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<Stateful<Div>> {
        if self.tab != Tab::Issues || self.issue_source != IssueSource::Linear {
            return None;
        }
        Some(
            self.look
                .button("linear-create-open", "New issue", false)
                .debug_selector(|| "linear-create-open".into())
                .on_click(cx.listener(|this, _, window, cx| {
                    this.open_linear_create(None, window, cx);
                })),
        )
    }

    /// The dialog's card, while it is open.
    pub(in crate::orchestrator::view) fn render_create(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let dialog = self.create.dialog.as_ref()?;
        let look = &self.look;
        let theme = &look.theme;
        let picked = self.create_team();
        let team = picked
            .as_ref()
            .map(|(options, index)| &options.teams[*index]);
        let header = div()
            .flex()
            .items_center()
            .gap_2()
            .child(self.render_chip(Menu::Team, team, cx))
            .child(look.icon("icons/chevron-right.svg", 10., theme.muted))
            .child(look.muted("New issue"))
            .child(div().flex_1())
            .child(
                look.icon_button("linear-create-close", "icons/close.svg", "Close")
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.close_overlays(window, cx);
                    })),
            );
        let properties = match (team, self.linear_options()) {
            (Some(team), _) => self.render_properties(team, cx),
            (None, Some(Err(error))) => div()
                .flex()
                .flex_wrap()
                .items_center()
                .gap_2()
                .child(
                    div()
                        .text_size(look.small())
                        .text_color(rgb(look.hue(RED)))
                        .child(error.to_string()),
                )
                .child(
                    look.button("linear-create-retry", "Try again", false)
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.open_linear_create(None, window, cx);
                        })),
                )
                .into_any_element(),
            (None, _) => look
                .muted("Reading your Linear teams\u{2026}")
                .into_any_element(),
        };
        let sending = dialog.sending.is_some();
        Some(
            card(look)
                .debug_selector(|| "linear-create".into())
                .w(px(WIDTH))
                .max_w_full()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _, _, cx| {
                        if let Some(dialog) =
                            this.create.dialog.as_mut().filter(|d| d.menu.is_some())
                        {
                            dialog.menu = None;
                            cx.notify();
                        }
                    }),
                )
                .child(header)
                .child(
                    div()
                        .px_1()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(dialog.title.clone()),
                )
                .child(dialog.description.clone())
                .child(properties)
                .when_some(dialog.problem.clone(), |el, problem| {
                    el.child(
                        div()
                            .debug_selector(|| "linear-create-problem".into())
                            .text_size(look.small())
                            .text_color(rgb(look.hue(RED)))
                            .child(problem),
                    )
                })
                .when_some(dialog.created.clone(), |el, created| {
                    el.child(look.muted(format!("Created {created}")))
                })
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_3()
                        .child(div().flex_1())
                        .child(self.render_more(dialog, cx))
                        .child(
                            look.button(
                                "linear-create-submit",
                                if sending {
                                    "Creating\u{2026}"
                                } else {
                                    "Create issue"
                                },
                                true,
                            )
                            .debug_selector(|| "linear-create-submit".into())
                            .on_click(cx.listener(|this, _, _, cx| this.submit_create(cx))),
                        ),
                )
                .into_any_element(),
        )
    }

    /// Status, Priority, Assignee, Project, Estimate, Labels, and Cycle;
    /// Estimate only in a team that estimates, Cycle only in one with
    /// cycles.
    fn render_properties(&self, team: &TeamOptions, cx: &mut Context<Self>) -> AnyElement {
        let mut menus = vec![Menu::Status, Menu::Priority, Menu::Assignee, Menu::Project];
        if team.estimates.is_some() {
            menus.push(Menu::Estimate);
        }
        menus.push(Menu::Labels);
        if !team.cycles.is_empty() {
            menus.push(Menu::Cycle);
        }
        div()
            .flex()
            .flex_wrap()
            .gap_2()
            .children(
                menus
                    .into_iter()
                    .map(|menu| self.render_chip(menu, Some(team), cx)),
            )
            .into_any_element()
    }

    /// `menu`'s chip, its picker under it while open.
    fn render_chip(
        &self,
        menu: Menu,
        team: Option<&TeamOptions>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let look = &self.look;
        let theme = &look.theme;
        let Some(dialog) = &self.create.dialog else {
            return div().into_any_element();
        };
        let (name, shown) = match team {
            Some(team) => (placeholder(menu), self.shown(menu, dialog, team)),
            None => ("Team", None),
        };
        let open = dialog.menu == Some(menu) && team.is_some();
        let hover = theme.active;
        let selector = format!("linear-create-chip-{}", menu.name());
        let set = shown.is_some();
        let (text, dot) = shown.unwrap_or_else(|| (name.to_owned(), None));
        let chip = div()
            .id(SharedString::from(selector.clone()))
            .debug_selector(move || selector.clone())
            .role(Role::Button)
            .aria_label(name)
            .flex_none()
            .flex()
            .items_center()
            .gap_1()
            .max_w(px(220.))
            .px_2()
            .py(px(3.))
            .rounded_full()
            .border_1()
            .border_color(rgb(if open { theme.primary() } else { theme.active }))
            .bg(rgb(if open {
                theme.primary_wash()
            } else {
                theme.surface
            }))
            .hover(move |style| style.bg(rgb(hover)))
            .cursor_pointer()
            .text_size(look.small())
            .text_color(rgb(if set { theme.foreground } else { theme.muted }))
            .children(dot.map(|color| look.dot(color, 8.)))
            .child(div().min_w_0().truncate().child(text))
            // On press, so the card's own press, which closes an open
            // picker, does not undo it.
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _, _, cx| {
                    cx.stop_propagation();
                    if let Some(dialog) = &mut this.create.dialog {
                        dialog.menu = (dialog.menu != Some(menu)).then_some(menu);
                        cx.notify();
                    }
                }),
            );
        let menu = team
            .filter(|_| open)
            .map(|team| self.render_menu(menu, team, cx));
        div()
            .relative()
            .flex_none()
            .child(chip)
            .when_some(menu, |el, menu| {
                el.child(div().absolute().top_full().left_0().child(menu))
            })
            .into_any_element()
    }

    /// What `menu`'s chip reads in `team`, `None` while unset.
    fn shown(&self, menu: Menu, dialog: &Dialog, team: &TeamOptions) -> Shown {
        let look = &self.look;
        let named = |choices: &[crate::orchestrator::linear::Choice], id: &Option<String>| {
            let id = id.as_deref()?;
            let choice = choices.iter().find(|choice| choice.id == id)?;
            Some((choice.name.clone(), None))
        };
        match menu {
            Menu::Team => Some((team.key.clone(), None)),
            Menu::Status => {
                let state = team.state(dialog.state.as_deref()?)?;
                Some((state.name.clone(), Some(self.status_hue(state))))
            }
            Menu::Priority => PRIORITIES
                .iter()
                .find(|(value, _)| *value == dialog.priority && *value != 0)
                .map(|(value, label)| ((*label).to_owned(), Some(priority_hue(look, *value)))),
            Menu::Assignee => named(&team.members, &dialog.assignee),
            Menu::Project => named(&team.projects, &dialog.project),
            Menu::Estimate => {
                let value = dialog.estimate?;
                let point = team.estimates.iter().flatten().find(|p| p.value == value)?;
                Some((point.label.clone(), None))
            }
            Menu::Labels => {
                let names: Vec<&str> = dialog
                    .labels
                    .iter()
                    .filter_map(|id| team.labels.iter().find(|label| label.id == *id))
                    .map(|label| label.name.as_str())
                    .collect();
                match names.as_slice() {
                    [] => None,
                    [only] => Some(((*only).to_owned(), None)),
                    [first, rest @ ..] => Some((format!("{first} +{}", rest.len()), None)),
                }
            }
            Menu::Cycle => {
                let id = dialog.cycle.as_deref()?;
                let choice = team.cycles.iter().find(|choice| choice.id == id)?;
                Some((choice.cycle.label(), None))
            }
        }
    }

    /// Create more: a switch, on while the dialog stays open for the next
    /// issue.
    fn render_more(&self, dialog: &Dialog, cx: &mut Context<Self>) -> Stateful<Div> {
        let look = &self.look;
        let theme = &look.theme;
        let on = dialog.more;
        div()
            .id("linear-create-more")
            .debug_selector(|| "linear-create-more".into())
            .role(Role::Switch)
            .aria_label("Create more")
            .aria_toggled(if on { Toggled::True } else { Toggled::False })
            .flex()
            .items_center()
            .gap_2()
            .cursor_pointer()
            .text_size(look.small())
            .text_color(rgb(theme.subtext()))
            .child(
                div()
                    .flex_none()
                    .w(px(26.))
                    .h(px(15.))
                    .p(px(2.))
                    .flex()
                    .when(on, |el| el.justify_end())
                    .rounded_full()
                    .bg(rgb(if on { theme.primary() } else { theme.active }))
                    .child(div().size(px(11.)).rounded_full().bg(rgb(if on {
                        theme.text_on(theme.primary())
                    } else {
                        theme.muted
                    }))),
            )
            .child("Create more")
            .on_click(cx.listener(|this, _, _, cx| {
                if let Some(dialog) = &mut this.create.dialog {
                    dialog.more = !dialog.more;
                    cx.notify();
                }
            }))
    }
}

/// What `menu`'s chip reads while unset.
fn placeholder(menu: Menu) -> &'static str {
    match menu {
        Menu::Assignee => "Assignee",
        Menu::Cycle => "Cycle",
        Menu::Estimate => "Estimate",
        Menu::Labels => "Labels",
        Menu::Priority => "Priority",
        Menu::Project => "Project",
        Menu::Status => "Status",
        Menu::Team => "Team",
    }
}
