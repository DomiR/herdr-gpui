//! `chat favorite` names a herdr-core pane by its chat key: `core:t1` on
//! this computer's core, `core@…` on another host's. Both find their tab.

use super::super::Host;
use crate::{
    browser::{Scope, TabId},
    chat::core_pane::key_on,
    core_terminals::panes::Panes,
    sidebar::layout_tests::fixture_window,
};
use gpui::TestAppContext;
use sheprd_backend::TerminalId;

#[gpui::test]
fn local_and_remote_core_keys_find_their_tab(cx: &mut TestAppContext) {
    let terminal = TerminalId::new("t1").unwrap();
    let local = TabId::test(1);
    let remote = TabId::test(2);
    let host = Host::Remote(Scope::endpoint("ssh:box"));
    let (view, cx) = cx.add_window_view(|window, cx| {
        let mut view = fixture_window(window, cx);
        for tab in [local, remote] {
            view.core_terminals.show_for_test(
                tab,
                Panes::single(terminal.clone()),
                false,
                Vec::new(),
            );
        }
        view.core_terminals.tabs.get_mut(&remote).unwrap().host = host.clone();
        view
    });
    cx.update(|_, cx| {
        let core = &view.read(cx).core_terminals;
        let local_key = key_on(&Host::Local, &terminal);
        let remote_key = key_on(&host, &terminal);
        assert_eq!(
            core.terminal_keyed(&local_key),
            Some((local, terminal.clone()))
        );
        assert_eq!(
            core.terminal_keyed(&remote_key),
            Some((remote, terminal.clone()))
        );
        let elsewhere = key_on(&Host::Remote(Scope::endpoint("ssh:other")), &terminal);
        assert_eq!(core.terminal_keyed(&elsewhere), None);
        assert_eq!(
            core.terminal_keyed("p1"),
            None,
            "a Herdr 1 pane is no core key"
        );
    });
}
