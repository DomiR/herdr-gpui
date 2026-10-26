//! The new issue dialog's pickers: one small menu per chip, hung under it,
//! listing what the dialog's team offers. Picking sets the chip and closes
//! the menu; Labels picks several, so it stays open.

use super::super::OrchestratorView;
use crate::{config::corners, orchestrator::linear::TeamOptions};
use gpui::{prelude::*, *};

/// Wider than a chip, so names fit.
const MENU_WIDTH: f32 = 240.;
/// Taller menus scroll.
const MENU_HEIGHT: f32 = 320.;

/// Linear's priorities, in its own pickers' order.
pub(super) const PRIORITIES: [(i64, &str); 5] = [
    (0, "No priority"),
    (1, "Urgent"),
    (2, "High"),
    (3, "Medium"),
    (4, "Low"),
];

/// A chip's picker.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Menu {
    // keep-sorted start
    Assignee,
    Cycle,
    Estimate,
    Labels,
    Priority,
    Project,
    Status,
    Team,
    // keep-sorted end
}

impl Menu {
    /// Its name in test selectors.
    pub(super) fn name(self) -> &'static str {
        match self {
            Self::Assignee => "assignee",
            Self::Cycle => "cycle",
            Self::Estimate => "estimate",
            Self::Labels => "labels",
            Self::Priority => "priority",
            Self::Project => "project",
            Self::Status => "status",
            Self::Team => "team",
        }
    }
}

/// What picking an entry sets.
#[derive(Clone, Debug)]
enum Pick {
    Assignee(Option<String>),
    Cycle(Option<String>),
    Estimate(Option<i64>),
    Label(String),
    Priority(i64),
    Project(Option<String>),
    State(String),
    Team(String),
}

/// One line of a menu: what it reads, the color of its dot, whether it is
/// the current choice, and what picking it sets.
struct Entry {
    label: String,
    dot: Option<u32>,
    on: bool,
    pick: Pick,
}

impl Entry {
    fn new(label: impl Into<String>, on: bool, pick: Pick) -> Self {
        Self {
            label: label.into(),
            dot: None,
            on,
            pick,
        }
    }

    fn dot(self, color: u32) -> Self {
        Self {
            dot: Some(color),
            ..self
        }
    }
}

impl OrchestratorView {
    /// What `menu` offers in `team`, the dialog's current choices on.
    fn entries(&self, menu: Menu, team: &TeamOptions) -> Vec<Entry> {
        let Some(dialog) = &self.create.dialog else {
            return Vec::new();
        };
        let look = &self.look;
        let none = |label: &str, on: bool, pick: Pick| Entry::new(label, on, pick);
        match menu {
            Menu::Team => self.options().map_or_else(Vec::new, |options| {
                options
                    .teams
                    .iter()
                    .map(|other| {
                        Entry::new(
                            format!("{} \u{00b7} {}", other.key, other.name),
                            other.id == team.id,
                            Pick::Team(other.id.clone()),
                        )
                    })
                    .collect()
            }),
            Menu::Status => team
                .states
                .iter()
                .filter_map(|state| {
                    let id = state.id.clone()?;
                    let on = dialog.state.as_deref() == Some(id.as_str());
                    Some(
                        Entry::new(state.name.clone(), on, Pick::State(id))
                            .dot(self.status_hue(state)),
                    )
                })
                .collect(),
            Menu::Priority => PRIORITIES
                .iter()
                .map(|(value, label)| {
                    Entry::new(*label, dialog.priority == *value, Pick::Priority(*value))
                        .dot(priority_hue(look, *value))
                })
                .collect(),
            Menu::Assignee => {
                let viewer = self.options().map(|options| options.viewer.clone());
                // The key owner first, as Linear lists them.
                let mut members: Vec<_> = team.members.iter().collect();
                members.sort_by_key(|member| Some(&member.id) != viewer.as_ref());
                std::iter::once(none(
                    "No assignee",
                    dialog.assignee.is_none(),
                    Pick::Assignee(None),
                ))
                .chain(members.into_iter().map(|member| {
                    let on = dialog.assignee.as_deref() == Some(member.id.as_str());
                    Entry::new(
                        member.name.clone(),
                        on,
                        Pick::Assignee(Some(member.id.clone())),
                    )
                }))
                .collect()
            }
            Menu::Project => std::iter::once(none(
                "No project",
                dialog.project.is_none(),
                Pick::Project(None),
            ))
            .chain(team.projects.iter().map(|project| {
                let on = dialog.project.as_deref() == Some(project.id.as_str());
                Entry::new(
                    project.name.clone(),
                    on,
                    Pick::Project(Some(project.id.clone())),
                )
            }))
            .collect(),
            Menu::Estimate => std::iter::once(none(
                "No estimate",
                dialog.estimate.is_none(),
                Pick::Estimate(None),
            ))
            .chain(team.estimates.iter().flatten().map(|point| {
                let label = match point.label.parse::<i64>() {
                    Ok(1) => "1 point".to_owned(),
                    Ok(_) => format!("{} points", point.label),
                    Err(_) => point.label.clone(),
                };
                Entry::new(
                    label,
                    dialog.estimate == Some(point.value),
                    Pick::Estimate(Some(point.value)),
                )
            }))
            .collect(),
            Menu::Labels => team
                .labels
                .iter()
                .map(|label| {
                    let color = label
                        .color
                        .map_or(look.theme.muted, |color| look.theme.ink(color));
                    Entry::new(
                        label.name.clone(),
                        dialog.labels.contains(&label.id),
                        Pick::Label(label.id.clone()),
                    )
                    .dot(color)
                })
                .collect(),
            Menu::Cycle => {
                std::iter::once(none("No cycle", dialog.cycle.is_none(), Pick::Cycle(None)))
                    .chain(team.cycles.iter().map(|choice| {
                        let mut label = choice.cycle.label();
                        if choice.cycle.active {
                            label.push_str(" (current)");
                        }
                        Entry::new(
                            label,
                            dialog.cycle.as_deref() == Some(choice.id.as_str()),
                            Pick::Cycle(Some(choice.id.clone())),
                        )
                    }))
                    .collect()
            }
        }
    }

    /// Sets what `pick` names, closing the menu unless it picks several.
    fn pick(&mut self, pick: Pick, cx: &mut Context<Self>) {
        if let Pick::Team(id) = &pick {
            self.pick_team(id);
        }
        let Some(dialog) = &mut self.create.dialog else {
            return;
        };
        match pick {
            Pick::Assignee(id) => dialog.assignee = id,
            Pick::Cycle(id) => dialog.cycle = id,
            Pick::Estimate(value) => dialog.estimate = value,
            Pick::Label(id) => {
                match dialog.labels.iter().position(|picked| *picked == id) {
                    Some(at) => {
                        dialog.labels.remove(at);
                    }
                    None => dialog.labels.push(id),
                }
                cx.notify();
                return;
            }
            Pick::Priority(value) => dialog.priority = value,
            Pick::Project(id) => dialog.project = id,
            Pick::State(id) => dialog.state = Some(id),
            Pick::Team(_) => {}
        }
        dialog.menu = None;
        cx.notify();
    }

    /// `menu`'s list for `team`, hung under its chip.
    pub(super) fn render_menu(
        &self,
        menu: Menu,
        team: &TeamOptions,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let look = &self.look;
        let theme = &look.theme;
        let hover = theme.active;
        let entries = self.entries(menu, team);
        let empty = entries.is_empty();
        let list = div()
            .id("linear-create-menu")
            .debug_selector(move || format!("linear-create-menu-{}", menu.name()))
            .occlude()
            .mt_1()
            .w(px(MENU_WIDTH))
            .max_h(px(MENU_HEIGHT))
            .overflow_y_scroll()
            .p_1()
            .flex()
            .flex_col()
            .rounded(px(corners::CONTROL))
            .bg(rgb(theme.surface))
            .border_1()
            .border_color(rgb(theme.active))
            .shadow_lg()
            .text_size(look.small())
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .when(empty, |el| {
                el.child(look.muted("Nothing to pick in this team").px_2().py_1())
            })
            .children(entries.into_iter().enumerate().map(|(index, entry)| {
                let selector = format!("linear-create-pick-{}-{}", menu.name(), entry.label);
                let Entry {
                    label,
                    dot,
                    on,
                    pick,
                } = entry;
                div()
                    .id(("linear-create-pick", index))
                    .debug_selector(move || selector.clone())
                    .role(Role::MenuItem)
                    .aria_label(label.clone())
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap_2()
                    .px_2()
                    .h(px(look.row(28.)))
                    .rounded(px(corners::SMALL))
                    .cursor_pointer()
                    .hover(move |style| style.bg(rgb(hover)))
                    .children(dot.map(|color| look.dot(color, 8.)))
                    .child(div().flex_1().min_w_0().truncate().child(label))
                    .when(on, |el| {
                        el.child(look.icon("icons/check.svg", 12., theme.primary()))
                    })
                    .on_click(cx.listener(move |this, _, _, cx| this.pick(pick.clone(), cx)))
            }));
        deferred(
            anchored()
                .anchor(Anchor::TopLeft)
                .snap_to_window()
                .child(list),
        )
        .with_priority(1)
        .into_any_element()
    }
}

/// The color of a Linear priority's dot: urgent hottest, none muted.
pub(super) fn priority_hue(look: &super::super::Look, priority: i64) -> u32 {
    use super::super::look::{BLUE, RED, YELLOW};
    match priority {
        1 => look.hue(RED),
        2 => look.hue(YELLOW),
        3 | 4 => look.hue(BLUE),
        _ => look.theme.muted,
    }
}
