use super::*;

fn on(server: &Server, login: &str, token: &str, organizations: &[&str]) -> Profile {
    let mut profile = profile(login, token, organizations);
    profile.server = server.clone();
    profile
}

fn acme() -> Server {
    Server::Enterprise("github.acme.com".into())
}

#[test]
fn enterprise_accounts_read_without_a_main_sign_in() {
    let acme = acme();
    let first = on(&acme, "octocat", "first-token", &[]);
    let platform = on(&acme, "bot", "platform-token", &["platform"]);
    let tokens = Tokens::new(None, [(&first, &[][..]), (&platform, &[][..])]);
    assert!(tokens.main().is_none());
    assert!(tokens.serves(&acme));
    assert_eq!(
        exposed(tokens.for_owner(&acme, "platform")),
        "platform-token"
    );
    assert_eq!(tokens.login_for(&acme, "platform"), Some("bot"));
    assert_eq!(
        tokens.reader(&acme, "platform").map(|(_, name)| name),
        Some("bot@github.acme.com")
    );
    // The first account listed stands in for an owner no account covers.
    assert_eq!(exposed(tokens.for_owner(&acme, "infra")), "first-token");
    assert_eq!(
        tokens.logins().collect::<Vec<_>>(),
        ["octocat@github.acme.com", "bot@github.acme.com"]
    );
}

#[test]
fn a_token_never_reads_another_server() {
    let acme = acme();
    let other = Server::Enterprise("git.example.org:8443".into());
    let main = profile("penso", "main-token", &[]);
    let ghe = on(&acme, "penso", "acme-token", &["rust-lang"]);
    let tokens = Tokens::new(Some(&main), [(&ghe, &[][..])]);
    // The same owner on GitHub.com stays with the GitHub.com sign-in.
    assert_eq!(
        exposed(tokens.for_owner(&Server::GitHub, "rust-lang")),
        "main-token"
    );
    assert_eq!(exposed(tokens.for_owner(&acme, "anyone")), "acme-token");
    assert!(tokens.for_owner(&other, "anyone").is_none());
    assert!(!tokens.serves(&other));
    // One login on two servers is two accounts.
    assert_eq!(
        tokens.logins().collect::<Vec<_>>(),
        ["penso", "penso@github.acme.com"]
    );

    let enterprise_only = Tokens::new(None, [(&ghe, &[][..])]);
    assert!(
        enterprise_only
            .for_owner(&Server::GitHub, "rust-lang")
            .is_none()
    );
}

#[test]
fn an_account_on_the_exact_port_wins_over_the_host_name() {
    let https = Server::Enterprise("ghe.example.com".into());
    let custom = Server::Enterprise("ghe.example.com:8443".into());
    let on_custom = on(&custom, "octocat", "custom-token", &[]);
    let on_https = on(&https, "octocat", "https-token", &[]);
    let tokens = Tokens::new(None, [(&on_custom, &[][..]), (&on_https, &[][..])]);
    assert_eq!(exposed(tokens.for_owner(&https, "acme")), "https-token");
    assert_eq!(exposed(tokens.for_owner(&custom, "acme")), "custom-token");
    // A remote that names no port, such as an SSH remote, still finds the host's only account.
    let custom_only = Tokens::new(None, [(&on_custom, &[][..])]);
    assert_eq!(
        exposed(custom_only.for_owner(&https, "acme")),
        "custom-token"
    );
}
