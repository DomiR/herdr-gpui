//! Favorite agents in the agents panel: listed first in any order, under
//! their own heading, with a star. Each host's daemon keeps its own
//! (`crate::agent_favorites`).

use super::listed::Listed;
use crate::direction::Logical;
use crate::i18n::fl;
use crate::sidebar::layout::SidebarLook;
use crate::{
    SheprdWindow,
    agent_favorites::{AgentFavorites, Starred},
};
use gpui::{prelude::*, *};

/// `rows` with the favorites first, each part in the order it had.
pub(crate) fn favorites_first<'a>(
    rows: Vec<(usize, Listed<'a>)>,
    starred: impl Fn(usize, &Listed<'a>) -> bool,
) -> Vec<(usize, Listed<'a>)> {
    let (mut favorites, rest): (Vec<_>, Vec<_>) = rows
        .into_iter()
        .partition(|(index, agent)| starred(*index, agent));
    favorites.extend(rest);
    favorites
}

impl SheprdWindow {
    /// Follows the favorites as their daemons answer.
    pub(crate) fn observe_agent_favorites(cx: &mut Context<Self>) -> Subscription {
        cx.observe_global::<AgentFavorites>(|this, cx| {
            this.agent_favorites = AgentFavorites::current(cx);
            cx.notify();
        })
    }

    /// Whether `agent`, of the host at `index`, is a favorite there.
    pub(crate) fn favorite_agent(&self, index: usize, agent: &Listed<'_>) -> bool {
        let Some(endpoint) = self.endpoints.get(index) else {
            return false;
        };
        agent
            .favorite_key()
            .is_some_and(|key| self.agent_favorites.contains(&endpoint.id, &key))
    }

    /// The panel's rows with the favorites first, when there are any.
    pub(crate) fn favorites_first<'a>(
        &self,
        rows: Vec<(usize, Listed<'a>)>,
    ) -> Vec<(usize, Listed<'a>)> {
        if !self.agent_favorites.any() {
            return rows;
        }
        favorites_first(rows, |index, agent| self.favorite_agent(index, agent))
    }

    /// Whether the Herdr 1 agent in `pane`, on the selected host, is a
    /// favorite there; `None` when no agent runs in it.
    pub(crate) fn pane_favorite(&self, pane: &str) -> Option<bool> {
        let snapshot = self.live.snapshot.as_ref()?;
        snapshot
            .agents
            .iter()
            .any(|agent| agent.pane_id == pane)
            .then(|| {
                let endpoint = &self.endpoints[self.selected_endpoint].id;
                self.agent_favorites.contains(endpoint, pane)
            })
    }

    /// The agent in `pane`, as `chat` keys panes: a Herdr 1 pane of the
    /// selected host, or a herdr-core terminal of this computer (`core:t7`)
    /// or another host (`core@…`). `None` when no agent runs there, or its
    /// core does not answer.
    pub(crate) fn starred_pane(&self, pane: &str) -> Option<Starred> {
        if pane.starts_with("core:") || pane.starts_with("core@") {
            let (tab, terminal) = self.core_terminals.terminal_keyed(pane)?;
            return self.core_pane_favorite(tab, &terminal);
        }
        let favorite = self.pane_favorite(pane)?;
        Some(Starred {
            endpoint: self.endpoints.get(self.selected_endpoint)?.id.clone(),
            key: pane.to_owned(),
            favorite,
        })
    }

    /// The heading over the favorites in the agents list.
    pub(in crate::sidebar) fn favorites_heading(&self, look: SidebarLook) -> Div {
        crate::sidebar::render::header(
            "agent-favorites",
            &fl!("agent-favorites-heading"),
            &self.config.sidebar,
            &self.theme,
            look,
        )
    }

    /// The rule under the favorites, over the other agents.
    pub(in crate::sidebar) fn favorites_rule(&self) -> Div {
        div()
            .debug_selector(|| "agent-favorites-rule".into())
            .flex_none()
            .my_1()
            .mx_2()
            .h(px(1.))
            .bg(rgb(self.theme.active))
    }

    /// The star at a favorite agent's row end, which unstars it; `None` for
    /// an agent that is no favorite. It sits over the row's top corner,
    /// clear of the leading rail and the status dot.
    pub(in crate::sidebar) fn agent_star(
        &self,
        index: usize,
        agent: &Listed<'_>,
        cx: &mut Context<Self>,
    ) -> Option<Stateful<Div>> {
        let endpoint = self.endpoints.get(index)?.id.clone();
        let key = agent.favorite_key()?;
        if !self.agent_favorites.contains(&endpoint, &key) {
            return None;
        }
        let label = fl!("agent-favorites-unstar", agent = agent.name());
        let selector = format!("agent-star-{endpoint}-{key}");
        Some(
            div()
                .id(SharedString::from(selector.clone()))
                .debug_selector(move || selector.clone())
                .role(Role::Button)
                .aria_label(label)
                .absolute()
                .top(px(2.))
                .inset_e(px(2.))
                .size(px(14.))
                .flex()
                .items_center()
                .justify_center()
                .cursor_pointer()
                .child(crate::icons::icon(
                    crate::icons::STARRED,
                    10.,
                    self.theme.palette[3],
                ))
                .on_click(cx.listener(move |_, _, _, cx| {
                    // The row under the star opens its agent on click.
                    cx.stop_propagation();
                    AgentFavorites::set(cx, &endpoint, key.clone(), false);
                })),
        )
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests;
