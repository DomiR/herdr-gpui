//! The Connect Linear dialog: where a personal API key comes from, a masked
//! field to paste one, and removing the saved key. The worker saves it; the
//! key's text is never logged or put in a `Debug` string.

use super::{Event, OrchestratorView, dispatch::card};
use crate::i18n::fl;
use crate::{
    config::corners,
    orchestrator::{Provider, SyncState, linear},
    search_input::SearchInput,
};
use gpui::{prelude::*, *};
use secrecy::SecretString;

/// Where Linear makes personal API keys.
const SETTINGS: &str = "https://linear.app/settings/account/security";

pub(crate) struct Dialog {
    pub(super) key: Entity<SearchInput>,
    pub(super) problem: Option<Problem>,
}

/// Why the dialog did not save, shown under the key.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Problem {
    /// The worker's queue is full.
    Busy,
    /// What was pasted is not a Linear API key.
    NotAKey,
}

impl Problem {
    fn text(self) -> String {
        match self {
            Self::Busy => fl!("orchestrator-linear-busy"),
            Self::NotAKey => fl!("orchestrator-linear-paste-key"),
        }
    }
}

impl OrchestratorView {
    /// The one repository whose worker syncs Linear: Linear is the user's,
    /// not a project's, so it is listed once however many projects are.
    pub(super) fn linear_repo(&self) -> Option<&super::Repo> {
        self.repos.iter().find(|repo| repo.request.linear.is_some())
    }

    pub(super) fn open_linear(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (ui, theme) = (self.look.ui.clone(), self.look.theme.clone());
        let key = cx.new(|cx| {
            let mut input = SearchInput::new(cx);
            input.set_placeholder("lin_api_\u{2026}", cx);
            input.set_masked(true, cx);
            input.set_appearance(ui, theme, cx);
            input
        });
        let focus = key.read(cx).focus.clone();
        self.linear = Some(Dialog { key, problem: None });
        window.focus(&focus, cx);
        cx.notify();
    }

    /// Saves the pasted key or, with `remove`, removes the saved one.
    pub(super) fn save_linear(
        &mut self,
        remove: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(dialog) = &mut self.linear else {
            return;
        };
        let key = if remove {
            None
        } else {
            let text = dialog.key.read(cx).text().trim().to_owned();
            if !linear::valid(&text) {
                dialog.problem = Some(Problem::NotAKey);
                cx.notify();
                return;
            }
            Some(SecretString::from(text))
        };
        let sent = self
            .linear_repo()
            .and_then(|repo| repo.service.as_ref())
            .is_some_and(|service| service.save_linear_key(key));
        let Some(dialog) = &mut self.linear else {
            return;
        };
        if !sent {
            dialog.problem = Some(Problem::Busy);
            cx.notify();
            return;
        }
        dialog.key.update(cx, SearchInput::clear);
        self.close_overlays(window, cx);
    }

    pub(super) fn render_linear(&self, dialog: &Dialog, cx: &mut Context<Self>) -> AnyElement {
        let look = &self.look;
        let theme = &look.theme;
        let sources = self.linear_repo().map(|repo| &repo.snapshot.sources);
        let connected = sources.into_iter().flatten().any(|status| {
            status.key.provider == Provider::Linear && !matches!(status.state, SyncState::SignIn)
        });
        // A cheap read of this process's environment, as the worker's is.
        let from_environment = std::env::var_os(linear::VARIABLE).is_some();
        card(look)
            .w(px(440.))
            .child(
                div()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(fl!("orchestrator-linear-connect")),
            )
            .child(
                div()
                    .text_size(look.small())
                    .text_color(rgb(theme.subtext()))
                    .child(fl!("orchestrator-linear-connect-hint")),
            )
            .child(
                div().flex().child(
                    look.button(
                        "orchestrator-linear-settings",
                        fl!("orchestrator-linear-open-settings"),
                        false,
                    )
                    .on_click(cx.listener(|_, _, _, cx| {
                        cx.emit(Event::OpenUrl(SETTINGS.into()));
                    })),
                ),
            )
            .child(
                div()
                    .px_2()
                    .py(px(4.))
                    .rounded(px(corners::CONTROL))
                    .bg(rgb(theme.background))
                    .border_1()
                    .border_color(rgb(theme.active))
                    .child(dialog.key.clone()),
            )
            .when(from_environment, |el| {
                el.child(look.muted(fl!("orchestrator-linear-variable-set")))
            })
            .when_some(dialog.problem, |el, problem| {
                el.child(
                    div()
                        .text_size(look.small())
                        .text_color(rgb(look.hue(super::look::RED)))
                        .child(problem.text()),
                )
            })
            .child(
                div()
                    .flex()
                    .gap_2()
                    .child(div().flex_1())
                    .child(
                        look.button(
                            "orchestrator-linear-cancel",
                            fl!("orchestrator-cancel"),
                            false,
                        )
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.close_overlays(window, cx);
                        })),
                    )
                    .when(connected, |el| {
                        el.child(
                            look.button(
                                "orchestrator-linear-remove",
                                fl!("orchestrator-linear-remove-key"),
                                false,
                            )
                            .on_click(cx.listener(
                                |this, _, window, cx| {
                                    this.save_linear(true, window, cx);
                                },
                            )),
                        )
                    })
                    .child(
                        look.button(
                            "orchestrator-linear-save",
                            fl!("orchestrator-linear-save"),
                            true,
                        )
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.save_linear(false, window, cx);
                        })),
                    ),
            )
            .into_any_element()
    }
}
