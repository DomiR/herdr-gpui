//! Starring a herdr-core agent from its pane's menu: favorites name its
//! terminal with its core's id (`control::favorites::core_pane`), on the
//! daemon of the host whose core runs it.

use super::Host;
use crate::{
    SheprdWindow,
    agent_favorites::Starred,
    browser::{TabId, scope},
};
use sheprd_backend::TerminalId;
use sheprd_remote_control::control::favorites::core_pane;

impl super::CoreTerminals {
    /// The tab and terminal a chat key names: `core:t7` on this computer's
    /// core, `core@…` on another host's (`chat::core_pane::key_on`).
    pub(crate) fn terminal_keyed(&self, key: &str) -> Option<(TabId, TerminalId)> {
        if !key.starts_with("core:") && !key.starts_with("core@") {
            return None;
        }
        self.tabs.iter().find_map(|(tab, shown)| {
            shown
                .panes
                .terminals()
                .into_iter()
                .find(|terminal| crate::chat::core_pane::key_on(&shown.host, terminal) == key)
                .map(|terminal| (*tab, terminal.clone()))
        })
    }
}

impl SheprdWindow {
    /// The key of the herdr-core pane holding the keyboard, its chat's or
    /// its terminal's, whether or not a chat view shows it.
    pub(crate) fn focused_core_key(&self, window: &gpui::Window, cx: &gpui::App) -> Option<String> {
        let (tab, terminal) = self
            .focused_core_chat(window, cx)
            .or_else(|| self.core_terminals.focused_pane(window, cx))?;
        Some(self.core_chat_key(tab, &terminal))
    }

    /// The agent in `terminal` of `tab` as favorites know it, or `None`
    /// when no agent runs there or its core does not answer, as then there
    /// is no core id to name it by.
    pub(crate) fn core_pane_favorite(&self, tab: TabId, terminal: &TerminalId) -> Option<Starred> {
        let host = self.core_terminals.host_of(tab);
        let core = &self
            .core_terminals
            .backend_of(&host)?
            .core()
            .hello()
            .core
            .id;
        let endpoint = self
            .endpoints
            .iter()
            .find(|endpoint| Host::of(&scope(endpoint)) == host)?;
        let session = self.core_terminals.session_of(&scope(endpoint))?;
        if !session
            .agents
            .iter()
            .any(|agent| agent.pane == terminal.as_str())
        {
            return None;
        }
        let key = core_pane(core, terminal.as_str());
        let favorite = self.agent_favorites.contains(&endpoint.id, &key);
        Some(Starred {
            endpoint: endpoint.id.clone(),
            key,
            favorite,
        })
    }
}
