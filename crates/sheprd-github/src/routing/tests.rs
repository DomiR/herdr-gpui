use super::*;

mod enterprise;

fn profile(login: &str, token: &str, organizations: &[&str]) -> Profile {
    Profile::fixture(login, Arc::new(token.into()), organizations)
}

fn exposed(token: Option<&ServerToken>) -> &str {
    token.map_or("", |token| {
        secrecy::ExposeSecret::expose_secret(&*token.token)
    })
}

#[test]
fn an_owner_no_additional_account_covers_is_read_with_the_main_account() {
    let main = profile("penso", "main-token", &[]);
    let emu = profile("fabien_microsoft", "emu-token", &["emu-org"]);
    let tokens = Tokens::new(Some(&main), [(&emu, &[][..])]);
    assert_eq!(
        exposed(tokens.for_owner(&Server::GitHub, "rust-lang")),
        "main-token"
    );
    assert_eq!(
        tokens.login_for(&Server::GitHub, "rust-lang"),
        Some("penso")
    );
    // The main account's own repositories are the main account's.
    assert_eq!(
        exposed(tokens.for_owner(&Server::GitHub, "penso")),
        "main-token"
    );
}

#[test]
fn an_additional_account_reads_its_login_organizations_and_listed_owners() {
    let main = profile("penso", "main-token", &[]);
    let emu = profile("fabien_microsoft", "emu-token", &["emu-org"]);
    let owners = vec!["listed-owner".to_owned()];
    let tokens = Tokens::new(Some(&main), [(&emu, owners.as_slice())]);
    for owner in [
        "fabien_microsoft",
        "FABIEN_MICROSOFT",
        "emu-org",
        "Listed-Owner",
    ] {
        assert_eq!(
            exposed(tokens.for_owner(&Server::GitHub, owner)),
            "emu-token",
            "{owner}"
        );
        assert_eq!(
            tokens.login_for(&Server::GitHub, owner),
            Some("fabien_microsoft")
        );
    }
}

#[test]
fn the_first_listed_account_covering_an_owner_wins() {
    let main = profile("penso", "main-token", &[]);
    let first = profile("first", "first-token", &["shared"]);
    let second = profile("second", "second-token", &["shared"]);
    let tokens = Tokens::new(Some(&main), [(&first, &[][..]), (&second, &[][..])]);
    assert_eq!(
        exposed(tokens.for_owner(&Server::GitHub, "shared")),
        "first-token"
    );
    assert_eq!(
        exposed(tokens.for_owner(&Server::GitHub, "second")),
        "second-token"
    );
}

#[test]
fn sameness_follows_token_allocations_and_coverage_not_token_text() {
    let main = profile("penso", "main-token", &[]);
    let emu = profile("emu", "emu-token", &[]);
    let tokens = Tokens::new(Some(&main), [(&emu, &[][..])]);
    assert!(tokens.same(&tokens.clone()));
    assert!(tokens.same(&Tokens::new(Some(&main), [(&emu, &[][..])])));
    // A renewed token is a new allocation, even with the same text.
    let renewed = profile("emu", "emu-token", &[]);
    assert!(!tokens.same(&Tokens::new(Some(&main), [(&renewed, &[][..])])));
    let owners = vec!["more".to_owned()];
    assert!(!tokens.same(&Tokens::new(Some(&main), [(&emu, owners.as_slice())])));
    assert!(!tokens.same(&Tokens::new(Some(&main), [])));
}

#[test]
fn the_key_names_every_account_main_first_with_the_owners_it_reads() {
    let main = profile("penso", "main-token", &[]);
    let emu = profile("emu", "emu-token", &["emu-org"]);
    assert_eq!(
        Tokens::new(Some(&main), [(&emu, &[][..])]).key(),
        "penso,emu(emu emu-org)"
    );
    assert_eq!(Tokens::new(Some(&main), []).key(), "penso");
    // Moving an owner to an account is another set, whose answers are its own.
    let owners = vec!["acme".to_owned()];
    assert_eq!(
        Tokens::new(Some(&main), [(&emu, owners.as_slice())]).key(),
        "penso,emu(emu emu-org acme)"
    );
    let tokens = Tokens::new(Some(&main), [(&emu, &[][..])]);
    assert_eq!(tokens.logins().collect::<Vec<_>>(), ["penso", "emu"]);
}

#[test]
fn a_lone_token_reads_every_owner() {
    let token: Arc<SecretString> = Arc::new("lone".into());
    let tokens = Tokens::from(token.clone());
    let read = tokens.for_owner(&Server::GitHub, "anyone");
    assert!(read.is_some_and(|read| Arc::ptr_eq(&read.token, &token)));
}

#[test]
fn no_github_account_reads_a_repository_on_another_server() {
    let main = profile("penso", "main-token", &[]);
    let emu = profile("emu", "emu-token", &["emu-org"]);
    let tokens = Tokens::new(Some(&main), [(&emu, &[][..])]);
    let enterprise = Server::Enterprise("github.example.com".into());
    for owner in ["penso", "emu", "emu-org", "rust-lang"] {
        assert!(tokens.for_owner(&enterprise, owner).is_none(), "{owner}");
        assert!(tokens.login_for(&enterprise, owner).is_none(), "{owner}");
    }
}
