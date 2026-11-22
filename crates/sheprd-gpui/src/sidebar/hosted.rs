//! What a host's own layout draws in place of these regions: the sidebar,
//! activity bar and status bar's data as plain JSON, and the actions they send
//! back. It reads the same state the regions do, and goes through the same
//! commands, so a host's row behaves as the window's own.

mod actions;
mod devices;

pub use actions::{Action, Drag, Menu};

use super::{
    ActivityPanel, SidebarMetrics,
    agents::{listed::Listed, status_style, status_text},
    layout::{HeaderCase, Highlight},
};
use crate::{config::Theme, preferences::AgentSort, system_load::Sample, window::SheprdWindow};
use gpui::App;
use serde_json::{Value, json};
use sheprd_client::protocol::{AgentStatus, ClientShellSnapshot};
use std::sync::Arc;

/// Backgrounds below this relative luminance, about mid gray, take light text.
const DARK_LUMINANCE: f32 = 0.2;

/// The window's state for a host's layout, in the parts that change apart, so
/// a host can run only what reads the part that changed. Plain data: it holds
/// nothing a mod could not show.
#[derive(Clone, Debug, PartialEq)]
pub struct HostedState {
    /// The machine's load, which moves every few seconds.
    pub load: Value,
    /// Workspaces, agents, panels and the connection.
    pub main: Value,
    /// The colors and font, for a host's own widgets to follow.
    pub palette: Palette,
    /// The colors, which move only with the theme.
    pub theme: Value,
}

/// Sheprd's theme and UI font, for a host's own widgets, so what it draws
/// around the window reads as part of it.
#[derive(Clone, Debug, PartialEq)]
pub struct Palette {
    pub accent: u32,
    pub active: u32,
    pub background: u32,
    pub cursor: u32,
    pub danger: u32,
    pub dark: bool,
    pub font_family: String,
    pub font_size: f32,
    pub foreground: u32,
    pub info: u32,
    pub muted: u32,
    pub sidebar: u32,
    /// The sidebar's own font, which the rows use rather than the UI's.
    pub sidebar_font_family: String,
    pub sidebar_font_size: f32,
    pub success: u32,
    pub surface: u32,
    pub warning: u32,
}

/// What a host's state is built from that the window itself changes. Cheap to
/// read on every notification, which terminal output sends once a frame; equal
/// stamps say the state need not be built. What changes without the window
/// noticing, such as the machine's load, is not in it.
#[derive(Clone)]
pub struct Stamp {
    agent_sort: AgentSort,
    agents_list: bool,
    /// The chat whose turns the layout shows; see `chat::turns`.
    chat_turns: Option<crate::chat::turns::TurnsStamp>,
    connected: bool,
    details_visible: bool,
    /// The device footer, and the picker while it is open, as a host's
    /// layout reads them: small, and built only while the window changes.
    device_picker: Value,
    /// Every device's snapshot and folds; see `hosted_devices_stamp`.
    devices: u64,
    /// How many merged worktrees the sidebar's broom would remove.
    merged: usize,
    metrics: SidebarMetrics,
    /// Whether Back and Forward fit the left column's header.
    navigation: bool,
    panel: ActivityPanel,
    section: bool,
    sidebar_visible: bool,
    snapshot: Option<Arc<ClientShellSnapshot>>,
    status_bar_visible: bool,
    theme: u32,
}

impl PartialEq for Stamp {
    fn eq(&self, other: &Self) -> bool {
        let same_snapshot = match (&self.snapshot, &other.snapshot) {
            (Some(left), Some(right)) => Arc::ptr_eq(left, right),
            (None, None) => true,
            _ => false,
        };
        same_snapshot
            && self.agent_sort == other.agent_sort
            && self.agents_list == other.agents_list
            && self.chat_turns == other.chat_turns
            && self.connected == other.connected
            && self.details_visible == other.details_visible
            && self.device_picker == other.device_picker
            && self.devices == other.devices
            && self.merged == other.merged
            && self.metrics == other.metrics
            && self.navigation == other.navigation
            && self.panel == other.panel
            && self.section == other.section
            && self.sidebar_visible == other.sidebar_visible
            && self.status_bar_visible == other.status_bar_visible
            && self.theme == other.theme
    }
}

fn status_name(status: AgentStatus) -> &'static str {
    match status {
        AgentStatus::Blocked => "blocked",
        AgentStatus::Done => "done",
        AgentStatus::Idle => "idle",
        AgentStatus::Unknown => "unknown",
        AgentStatus::Working => "working",
    }
}

fn panel_id(panel: ActivityPanel) -> &'static str {
    match panel {
        ActivityPanel::Automations => "automations",
        ActivityPanel::Issues => "issues",
        ActivityPanel::Overview => "overview",
        ActivityPanel::Ports => "ports",
        ActivityPanel::PullRequests => "pull_requests",
        ActivityPanel::Security => "security",
        ActivityPanel::Tasks => "tasks",
        ActivityPanel::Usage => "usage",
        ActivityPanel::Traces => "traces",
        ActivityPanel::Workspaces => "workspaces",
    }
}

/// An agent as the window's agents list labels it: the place it runs (its
/// workspace, and its tab when that earns a place), its pane's name, and the
/// word its integration chose for its status.
fn agent_state(
    agent: &Listed<'_>,
    show_state: bool,
    (endpoint, host): (&str, Option<&str>),
) -> Value {
    let place = agent.place();
    let status = status_text(agent.status());
    let title = match agent {
        Listed::Herdr1 { agent, .. } => agent.title.as_deref(),
        Listed::Core(_) => None,
    };
    json!({
        "core": agent.is_core(),
        "endpoint": endpoint,
        "focused": agent.focused(),
        "host": host,
        "kind": agent.identity(),
        "name": agent.name(),
        "pane": agent.id(),
        "pane_name": agent.pane(),
        "state": show_state.then(|| agent.state_label(&status)),
        "status": status_name(agent.status()),
        "tab": place.and_then(|(_, tab)| tab),
        "title": title,
        "workspace": agent.workspace(),
        "workspace_label": place.map(|(workspace, _)| workspace),
    })
}

/// Moonframe's named text sizes and their scale of its base, the UI size.
const TEXT_STEPS: [(&str, f32); 7] = [
    ("small", 0.85),
    ("normal", 1.0),
    ("large", 1.2),
    ("title", 1.5),
    ("headline", 1.75),
    ("display", 2.25),
    ("hero", 3.0),
];

/// The named size nearest `size` over Moonframe's `base`: how the sidebar's
/// own font size reaches Luau, which names sizes rather than pixels.
fn text_step(size: f32, base: f32) -> &'static str {
    let ratio = size / base;
    TEXT_STEPS
        .iter()
        .min_by(|(_, a), (_, b)| (a - ratio).abs().total_cmp(&(b - ratio).abs()))
        .map_or("normal", |(name, _)| name)
}

/// The configured layout's name and the spacing it and the config's
/// overrides select, as the window's own rows read them.
fn layout_state(name: &str, metrics: &SidebarMetrics) -> Value {
    json!({
        "child_details": metrics.child_details,
        "child_indent": metrics.child_indent,
        "gap": metrics.gap,
        "name": name,
        // How far rows step in under a device's header, as several devices
        // list them; the sidebar has no tag sections to add theirs.
        "nest": metrics.nest,
        "outline": metrics.highlight == Highlight::Outline,
        "padding": metrics.padding,
        "pr_counts": metrics.pr_counts,
        "radius": metrics.radius,
        "row_padding": metrics.row_padding,
        "inset": metrics.inset,
        "spacing": metrics.spacing,
        "style_padding": metrics.style_padding,
        "title_case": metrics.header_case == HeaderCase::Title,
        "tree_lines": metrics.tree_lines,
        "workspace_details": metrics.workspace_details,
    })
}

/// Each status's color, as the window's status dots draw it.
fn status_colors(theme: &Theme) -> Value {
    let color = |status| hex(status_style(status, theme).2);
    json!({
        "blocked": color(AgentStatus::Blocked),
        "done": color(AgentStatus::Done),
        "idle": color(AgentStatus::Idle),
        "unknown": color(AgentStatus::Unknown),
        "working": color(AgentStatus::Working),
    })
}

/// `color` as `#rrggbb`, how a mod's style names one.
pub(crate) fn hex(color: u32) -> String {
    format!("#{:06x}", color & 0xff_ffff)
}

/// Whether `color` is a dark background.
fn is_dark(color: u32) -> bool {
    crate::contrast::luminance(color) < DARK_LUMINANCE
}

/// The build's worktree, when it was built from one, as the window's foot
/// names it: its branch and pull request.
fn build_state() -> Value {
    json!({
        "branch": env!("HERDR_BUILD_BRANCH"),
        "pr": Some(env!("HERDR_BUILD_PR")).filter(|pr| !pr.is_empty()),
        "worktree": env!("HERDR_BUILD_WORKTREE") == "1",
    })
}

fn load_state(sample: Option<&Sample>) -> Value {
    json!({
        "cpu": sample.and_then(|sample| sample.cpu),
        "memory": sample.and_then(|sample| sample.memory).map(|memory| memory.percent()),
    })
}

impl SheprdWindow {
    /// Whether a host's state may have changed; see [`Stamp`].
    pub(crate) fn hosted_stamp(&self) -> Stamp {
        Stamp {
            agent_sort: self.agent_sort,
            agents_list: self.hosted_agents_list(),
            chat_turns: self.hosted_chat_turns_stamp(),
            connected: self.live.status.is_connected(),
            details_visible: self.details_visible,
            device_picker: self.hosted_device_picker(),
            devices: self.hosted_devices_stamp(),
            merged: self.merged_worktrees().len(),
            panel: self.activity_panel(),
            section: self.activity_section().is_some(),
            metrics: self.hosted_metrics(),
            navigation: self.navigation_fits_sidebar(self.viewport_width),
            sidebar_visible: self.sidebar_visible,
            snapshot: self.live.snapshot.clone(),
            status_bar_visible: self.status_bar_visible,
            theme: self.theme.background,
        }
    }

    /// The configured layout's spacing, with the config's overrides and
    /// worktree guides, as the window's own sidebar lays out.
    fn hosted_metrics(&self) -> SidebarMetrics {
        SidebarMetrics::for_mode(self.config.layout.mode)
            .with(&self.config.sidebar_style.overrides)
            .with_guides(self.config.worktrees.sidebar)
    }

    /// Every listed device's agents in the order the agents list paints them,
    /// each with its device, and the device's name when several are listed.
    fn hosted_agents(&self) -> Vec<Value> {
        let multi = self.endpoints.len() > 1;
        self.panel_agents()
            .into_iter()
            .filter_map(|(index, agent)| {
                let endpoint = self.endpoints.get(index)?;
                let shows = self
                    .config
                    .sidebar_layout
                    .agents
                    .shows_status_text(agent.identity());
                let host = multi.then_some(endpoint.label.as_str());
                Some(agent_state(&agent, shows, (&endpoint.id, host)))
            })
            .collect()
    }

    /// Whether the sidebar shows the agents list: agents shown, and listed
    /// apart rather than under their workspaces.
    fn hosted_agents_list(&self) -> bool {
        self.config.show_agents
            && self.config.agents_placement == crate::config::AgentsPlacement::Panel
    }

    /// The window's state for a host's layout.
    pub(crate) fn hosted_state(&self, cx: &App) -> HostedState {
        let counts = self.activity_counts(cx);
        let panels: Vec<Value> = ActivityPanel::ALL
            .iter()
            .copied()
            .filter(|panel| self.activity_panel_listed(*panel))
            .map(|panel| {
                json!({
                    "count": counts.of(panel).filter(|count| *count > 0),
                    // Drawn in the accent while it holds something unseen,
                    // as the window's own bubble is.
                    "unseen": self.activity.unseen(panel, counts.latest(panel)),
                    "id": panel_id(panel),
                    // Sheprd's own icon, as its bar draws it.
                    "icon": crate::icons::hosted::app_name(super::activity::icon(panel)),
                    "label": super::activity::label(panel).to_string(),
                })
            })
            .collect();
        let theme = &self.theme;
        HostedState {
            load: self.hosted_load(),
            palette: Palette {
                accent: theme.primary(),
                active: theme.active,
                background: theme.background,
                cursor: theme.cursor,
                danger: theme.palette[1],
                dark: is_dark(theme.background),
                font_family: self.config.ui.family.clone(),
                font_size: self.config.ui.size,
                foreground: theme.foreground,
                info: theme.palette[6],
                muted: theme.muted,
                sidebar: theme.sidebar_background(),
                sidebar_font_family: self.config.sidebar.family.clone(),
                sidebar_font_size: self.config.sidebar.size,
                success: theme.palette[2],
                surface: theme.surface,
                warning: theme.palette[3],
            },
            main: json!({
                "agents": self.hosted_agents(),
                // Whether the sidebar lists agents in a list of their own,
                // as the window's own sidebar does.
                "agents_list": self.hosted_agents_list(),
                "agent_sort": match self.agent_sort {
                    AgentSort::Grouped => "grouped",
                    AgentSort::Priority => "priority",
                },
                "chat_turns": self.hosted_chat_turns(),
                // Merged worktrees the spaces heading's broom removes.
                "merged": self.merged_worktrees().len(),
                "layout": layout_state(self.config.layout.mode.name(), &self.hosted_metrics()),
                "connected": self.live.status.is_connected(),
                "details_visible": self.details_visible,
                "panel": panel_id(self.activity_panel()),
                "panels": panels,
                "section": self.activity_section().is_some(),
                "navigation_in_header": self.navigation_fits_sidebar(self.viewport_width),
                "sidebar_visible": self.sidebar_visible,
                "status": self.live.status_text(self.local_error.as_deref()),
                "status_bar_visible": self.status_bar_visible,
                "usage": self.hosted_usage(),
            "workspaces": self.hosted_workspaces(cx),
                "devices": self.hosted_devices(cx),
                "device_picker": self.hosted_device_picker(),
                "multi": self.endpoints.len() > 1,
                "build": build_state(),
            }),
            theme: json!({
                "accent": hex(self.theme.primary()),
                "active": hex(self.theme.active),
                "added": hex(self.theme.ink(self.theme.palette[2])),
                "background": hex(self.theme.background),
                "danger": hex(self.theme.palette[1]),
                // The uncommitted-work chip's hue, and its pencil's.
                "dirty": hex(self.theme.palette[3]),
                "dirty_ink": hex(self.theme.ink(self.theme.palette[3])),
                "foreground": hex(self.theme.foreground),
                "muted": hex(self.theme.muted),
                "on_accent": hex(self.theme.text_on(self.theme.primary())),
                "on_muted": hex(self.theme.text_on(self.theme.muted)),
                "removed": hex(self.theme.ink(self.theme.palette[1])),
                "sidebar": hex(self.theme.sidebar_background()),
                // The sidebar's font size, as Moonframe names sizes over the
                // UI size its text starts from.
                "sidebar_size": text_step(self.config.sidebar.size, self.config.ui.size),
                "subtext": hex(self.theme.subtext()),
                "success": hex(self.theme.palette[2]),
                "surface": hex(self.theme.surface),
                "status": status_colors(&self.theme),
                "warning": hex(self.theme.palette[3]),
            }),
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests;
