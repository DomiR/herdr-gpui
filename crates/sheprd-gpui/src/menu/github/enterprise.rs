//! Adding a GitHub Enterprise Server account on the GitHub page: its server
//! and the pasted token it signs in with, in one form. Device codes, the
//! GitHub App, and the GitHub CLI exist on GitHub.com only, so none is offered.

use super::Adding;
use crate::i18n::fl;
use crate::{
    SheprdWindow,
    config::{Config, EnterpriseAccount},
    search_input::SearchInput,
};
use gpui::{prelude::*, *};
use secrecy::SecretString;
use sheprd_github::Server;

/// The fields of an Enterprise account being added.
pub(crate) struct EnterpriseForm {
    server: Entity<SearchInput>,
    token: Entity<SearchInput>,
}

impl EnterpriseForm {
    /// The field holding the keyboard: the token's, or else the server's.
    pub(super) fn focused(&self, window: &Window, cx: &App) -> &Entity<SearchInput> {
        if self.token.read(cx).focus.is_focused(window) {
            &self.token
        } else {
            &self.server
        }
    }

    /// Moves the keyboard to the other field.
    pub(super) fn switch(&self, window: &mut Window, cx: &mut App) {
        let next = if self.token.read(cx).focus.is_focused(window) {
            &self.server
        } else {
            &self.token
        };
        let focus = next.read(cx).focus.clone();
        window.focus(&focus, cx);
    }
}

impl SheprdWindow {
    /// Whether another account fits beside the configured ones.
    pub(super) fn can_add_enterprise_account(&self) -> bool {
        self.menu.github_accounts.len() < crate::config::MAX_ACCOUNTS
    }

    pub(super) fn open_github_server(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let field = |masked: bool, placeholder: fn() -> String, cx: &mut Context<Self>| {
            let input = cx.new(SearchInput::new);
            input.update(cx, |input, cx| {
                input.set_masked(masked, cx);
                input.set_placeholder_with(placeholder, cx);
                input.set_appearance(self.config.ui.clone(), self.theme.clone(), cx);
            });
            input
        };
        let server = field(false, || fl!("github-menu-server-placeholder"), cx);
        let token = field(true, || fl!("github-menu-token-placeholder"), cx);
        let focus = server.read(cx).focus.clone();
        window.focus(&focus, cx);
        self.close_github_token();
        self.menu.error = None;
        self.menu.github_server = Some(EnterpriseForm { server, token });
    }

    pub(super) fn close_github_server(&mut self) {
        self.menu.github_server = None;
    }

    /// Add an account on the typed server and sign it in with the typed
    /// token once its config entry loads. The config edit runs off the UI
    /// thread. An empty field takes the keyboard instead, so Enter in the
    /// server field moves on to the token.
    pub(super) fn submit_github_server(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(form) = self.menu.github_server.as_ref() else {
            return;
        };
        let text = form.server.read(cx).text().trim().to_owned();
        let token = form.token.read(cx).text().trim().to_owned();
        if let Some(empty) = [(&text, &form.server), (&token, &form.token)]
            .into_iter()
            .find_map(|(value, input)| value.is_empty().then_some(input))
        {
            let focus = empty.read(cx).focus.clone();
            window.focus(&focus, cx);
            return;
        }
        let Some(server) = Server::enterprise(&text) else {
            self.menu.error = Some(fl!("github-menu-server-invalid"));
            return;
        };
        if !self.can_add_enterprise_account() {
            self.menu.error = Some(fl!(
                "github-menu-too-many-accounts",
                count = crate::config::MAX_ACCOUNTS
            ));
            return;
        }
        let account = match EnterpriseAccount::generate(&server) {
            Ok(account) => account,
            Err(error) => {
                self.menu.error = Some(error.to_string());
                return;
            }
        };
        tracing::info!(
            category = "github_accounts",
            "Adding a GitHub Enterprise Server account"
        );
        self.close_github_server();
        window.focus(&self.menu.focus, cx);
        self.menu.error = None;
        self.menu.github_adding = Some(Adding {
            id: account.id.clone(),
            token: Some(SecretString::from(token)),
        });
        self.write_preference(move || Config::add_enterprise_account(&account), cx);
    }

    /// The form and what each field takes, while it is open.
    pub(super) fn render_github_server(&self, cx: &App) -> Option<Div> {
        let form = self.menu.github_server.as_ref()?;
        let host = form.server.read(cx).text().trim().to_owned();
        let server = if host.is_empty() {
            fl!("github-menu-server-placeholder")
        } else {
            host
        };
        let help = |text: String| div().text_color(rgb(self.theme.muted)).child(text);
        Some(
            div()
                .debug_selector(|| "github-server".into())
                .mt(px(12.))
                .flex()
                .flex_col()
                .gap(px(6.))
                .child(fl!("github-menu-server-label"))
                .child(form.server.clone())
                .child(help(fl!("github-menu-server-help")))
                .child(div().mt(px(6.)).child(fl!("github-menu-token-label")))
                .child(form.token.clone())
                .child(help(fl!(
                    "github-menu-enterprise-token-help",
                    server = server
                ))),
        )
    }
}
