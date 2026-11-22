//! Security reads GitHub.com only: an Enterprise Server account alone asks
//! for a GitHub.com sign-in rather than reading with the wrong server.
use super::*;
use crate::{
    github::{Account, Auth, Profile, Server},
    i18n::fl,
    menu::AdditionalAccount,
};

const ID: &str = "0123456789abcdef0123456789abcdef";

fn enterprise_account() -> AdditionalAccount {
    let server = Server::Enterprise("github.acme.com".into());
    let mut auth = Auth::for_account(Account::enterprise(&server, ID).unwrap());
    let mut profile = Profile::fixture("octocat", Arc::new("acme-token".into()), &[]);
    profile.server = server.clone();
    auth.profile = Some(profile);
    AdditionalAccount {
        auth,
        id: ID.into(),
        owners: Vec::new(),
        server,
    }
}

#[gpui::test]
fn an_enterprise_account_alone_asks_for_a_github_sign_in(cx: &mut gpui::TestAppContext) {
    let (view, cx) = section::window(cx);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.menu.github_accounts = vec![enterprise_account()];
            assert!(view.main_tokens().is_some());
            view.poll_security(cx);
            assert!(!view.security.busy());
            assert_eq!(view.security_empty(), fl!("security-sign-in"));

            view.menu.github.profile =
                Some(Profile::fixture("penso", Arc::new("token".into()), &[]));
            assert_eq!(view.security_empty(), fl!("security-no-repository"));
        });
    });
}
