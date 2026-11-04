//! The document's owning window resolves targets and reveals the agent tab.
// i18n: migrated

use super::{Error, Mode, Result, UnsupportedReason, links, runtime::Enable};
use crate::{
    HerdrWindow,
    browser::{self, GroupId, Location, Pick, Scope, Shown, Store, TabId},
    doc_view::{DocEditor, Docs},
    i18n::Localize,
    teleport::Host,
    window::Flash,
};
use gpui::{Context, Entity, Window, WindowId};
use herdr_client::ConnectTarget;
use std::path::Path;

#[derive(Clone)]
pub(super) struct Reveal {
    pub(super) doc: TabId,
    pub(super) group: GroupId,
    pub(super) scope: Scope,
    pub(super) window: WindowId,
    pub(super) workspace: String,
}

pub(super) fn host(target: &ConnectTarget) -> Result<Host> {
    if !matches!(
        target,
        ConnectTarget::Local
            | ConnectTarget::Session {
                development: false,
                ..
            }
    ) {
        return Err(Error::Unsupported {
            reason: UnsupportedReason::Target,
        });
    }
    crate::pane_move::host_for(target).map_err(|source| Error::Unsupported {
        reason: UnsupportedReason::Host(source),
    })
}

pub(crate) fn toggle(
    view: &mut HerdrWindow,
    editor: &Entity<DocEditor>,
    window: &mut Window,
    cx: &mut Context<HerdrWindow>,
) {
    let Some(links) = links(cx) else { return };
    let id = editor.read(cx).tab();
    let Some(tab) = cx
        .try_global::<Store>()
        .and_then(|store| store.get(id))
        .cloned()
    else {
        return;
    };
    let Some(Location::Doc { file }) = tab.location else {
        return;
    };
    let path = Path::new(&file.path);
    if links
        .read(cx)
        .status(path)
        .is_some_and(|status| status.mode != Mode::Off)
    {
        links.update(cx, |links, cx| links.disable(path, cx));
        return;
    }
    let result = (|| {
        let endpoint = view
            .endpoints
            .iter()
            .find(|endpoint| browser::scope(endpoint) == tab.scope)
            .ok_or(Error::Unavailable)?;
        host(&endpoint.connection.target)?;
        let snapshot = endpoint
            .online_since()
            .and(endpoint.live.latest_snapshot())
            .filter(|snapshot| {
                snapshot
                    .workspaces
                    .iter()
                    .any(|workspace| workspace.workspace_id == tab.workspace_id)
            })
            .cloned()
            .ok_or(Error::Unavailable)?;
        let target = endpoint.connection.target.clone();
        let reveal = view
            .group_slots()
            .into_iter()
            .find(|slot| matches!(view.group_shown(slot.id, cx), Shown::Page(page) if page == id))
            .map(|slot| Reveal {
                doc: id,
                group: slot.id,
                scope: tab.scope.clone(),
                window: window.window_handle().window_id(),
                workspace: tab.workspace_id.clone(),
            });
        let buffer = Docs::open(path, cx);
        links.update(cx, |links, cx| {
            links.enable(
                Enable {
                    buffer,
                    reveal,
                    scope: tab.scope,
                    snapshot,
                    target,
                    workspace_id: tab.workspace_id,
                },
                cx,
            )
        })
    })();
    if let Err(error) = result {
        view.show_flash(Flash::warning(error.localize()), cx);
    }
}

pub(crate) fn poll(view: &mut HerdrWindow, window: &mut Window, cx: &mut Context<HerdrWindow>) {
    let Some(links) = links(cx) else { return };
    links.update(cx, |links, cx| {
        for endpoint in &view.endpoints {
            links.observe_endpoint(
                &browser::scope(endpoint),
                endpoint.online_since(),
                endpoint.live.latest_snapshot(),
                cx,
            );
        }
        links.poll_storage(cx);
    });
    for (id, editor) in &view.doc_views {
        let supported = cx
            .try_global::<Store>()
            .and_then(|store| store.get(*id))
            .and_then(|tab| {
                view.endpoints
                    .iter()
                    .find(|endpoint| browser::scope(endpoint) == tab.scope)
            })
            .is_some_and(|endpoint| host(&endpoint.connection.target).is_ok());
        editor.update(cx, |editor, cx| editor.set_link_supported(supported, cx));
    }
    let requests = links
        .read(cx)
        .reveal_requests(window.window_handle().window_id());
    for (path, reveal, record) in requests {
        let Some(record) = record else { continue };
        let valid = view.browser_key() == Some((reveal.scope.clone(), reveal.workspace.clone()))
            && cx
                .try_global::<Store>()
                .is_some_and(|store| store.get(reveal.doc).is_some())
            && view
                .group_slots()
                .iter()
                .any(|slot| slot.id == reveal.group)
            && matches!(view.group_shown(reveal.group, cx), Shown::Page(id) if id == reveal.doc);
        if !valid {
            links.update(cx, |links, _| links.forget_reveal(&path));
            continue;
        }
        let Some(snapshot) = &view.live.snapshot else {
            continue;
        };
        let Some(tab) = snapshot.tabs.iter().find(|tab| tab.tab_id == record.tab_id) else {
            continue;
        };
        if tab.workspace_id != reveal.workspace {
            links.update(cx, |links, _| links.forget_reveal(&path));
            continue;
        }
        let existing = view
            .group_slots()
            .into_iter()
            .find(|slot| view.group_pick(slot.id) == Some(Pick::Herdr(record.tab_id.clone())))
            .map(|slot| slot.id);
        if existing.is_none() {
            view.split_group(reveal.group, window, cx);
            if let Some(group) = view.active_group().filter(|group| *group != reveal.group) {
                view.choose_herdr_tab(group, &record.tab_id, window, cx);
            }
        }
        view.show_browser_tab_in(Some(reveal.group), reveal.doc, window, cx);
        if let Some(editor) = view.doc_views.get(&reveal.doc) {
            let focus = editor.read(cx).focus.clone();
            window.focus(&focus, cx);
        }
        links.update(cx, |links, _| links.forget_reveal(&path));
    }
}
