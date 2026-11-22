//! Which signed-in account reads a repository, on the repository's server
//! only: an account when the owner is its login, one of its organizations,
//! or an owner its config lists; otherwise the main sign-in on GitHub.com,
//! and the first account listed on an Enterprise Server. Workers learn a
//! checkout's repository from its remote, so they carry every token and
//! choose once they know it.

use super::{Profile, Server};
use secrecy::SecretString;
use std::sync::Arc;

/// A token and the server its account signed in to. Requests made with it
/// go to that server's API only.
#[derive(Clone)]
pub struct ServerToken {
    pub server: Server,
    pub token: Arc<SecretString>,
}

impl ServerToken {
    pub fn new(server: Server, token: Arc<SecretString>) -> Self {
        Self { server, token }
    }

    /// A GitHub.com token.
    pub fn github(token: Arc<SecretString>) -> Self {
        Self {
            server: Server::GitHub,
            token,
        }
    }
}

/// One account's token and the owners it reads; the main account's list is
/// empty because it reads every owner on its server no other account claims.
struct Route {
    login: String,
    /// The login, `login@host` on an Enterprise Server, so the same login on
    /// two servers stays two accounts.
    name: String,
    token: ServerToken,
    owners: Vec<String>,
    main: bool,
}

impl Route {
    fn covers(&self, owner: &str) -> bool {
        self.owners
            .iter()
            .any(|covered| covered.eq_ignore_ascii_case(owner))
    }
}

/// The tokens a window reads GitHub with. Cheap to clone.
#[derive(Clone)]
pub struct Tokens(Arc<[Route]>);

impl Tokens {
    /// `main` for every GitHub.com owner, except those another account on
    /// GitHub.com covers: its login, its organizations, and `owners` from its
    /// config. The first listed account covering an owner wins. An account on
    /// an Enterprise Server reads that server's owners the same way, the
    /// first one listed standing in for a main sign-in.
    pub fn new<'a>(
        main: Option<&Profile>,
        additional: impl IntoIterator<Item = (&'a Profile, &'a [String])>,
    ) -> Self {
        let main = main.map(|main| Route {
            login: main.login.clone(),
            name: main.server.account_name(&main.login),
            token: main.server_token(),
            owners: Vec::new(),
            main: true,
        });
        let routes = main
            .into_iter()
            .chain(additional.into_iter().map(|(profile, owners)| {
                Route {
                    login: profile.login.clone(),
                    name: profile.server.account_name(&profile.login),
                    token: profile.server_token(),
                    owners: std::iter::once(&profile.login)
                        .chain(profile.organizations.iter().flatten())
                        .chain(owners)
                        .cloned()
                        .collect(),
                    main: false,
                }
            }));
        Self(routes.collect())
    }

    /// An account on the remote's exact host and port goes first. The host
    /// name alone matches only when no such account exists.
    fn route(&self, server: &Server, owner: &str) -> Option<&Route> {
        self.route_among(server, owner, |route| route.token.server == *server)
            .or_else(|| self.route_among(server, owner, |route| route.token.server.serves(server)))
    }

    fn route_among(
        &self,
        server: &Server,
        owner: &str,
        on: impl Fn(&Route) -> bool,
    ) -> Option<&Route> {
        let mut on_server = self.0.iter().filter(|route| on(route));
        on_server
            .clone()
            .find(|route| !route.main && route.covers(owner))
            .or_else(|| on_server.find(|route| route.main || server.is_enterprise()))
    }

    /// The token that reads repositories owned by `owner` on `server`, the
    /// server a remote names, when an account there does.
    pub fn for_owner(&self, server: &Server, owner: &str) -> Option<&ServerToken> {
        self.route(server, owner).map(|route| &route.token)
    }

    /// The same for the server at `host`, a remote's host name.
    pub fn for_host(&self, host: &str, owner: &str) -> Option<&ServerToken> {
        self.for_owner(&Server::of_host(host)?, owner)
    }

    /// The main sign-in's token, when there is one.
    pub fn main(&self) -> Option<&ServerToken> {
        self.0
            .iter()
            .find(|route| route.main)
            .map(|route| &route.token)
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Whether some account signed in to the server `server` names.
    pub fn serves(&self, server: &Server) -> bool {
        self.0.iter().any(|route| route.token.server.serves(server))
    }

    /// The login of the account that reads repositories owned by `owner` on
    /// `server`.
    pub fn login_for(&self, server: &Server, owner: &str) -> Option<&str> {
        self.route(server, owner).map(|route| route.login.as_str())
    }

    /// The token [`Tokens::for_owner`] gives, and its account by the name
    /// [`Tokens::logins`] gives it.
    pub fn reader(&self, server: &Server, owner: &str) -> Option<(&ServerToken, &str)> {
        self.route(server, owner)
            .map(|route| (&route.token, route.name.as_str()))
    }

    /// The same accounts, owners, and token allocations. A renewed token is a
    /// new allocation, which is how caches notice it, never by its text.
    pub fn same(&self, other: &Self) -> bool {
        self.0.len() == other.0.len()
            && self.0.iter().zip(other.0.iter()).all(|(this, that)| {
                Arc::ptr_eq(&this.token.token, &that.token.token)
                    && this.token.server == that.token.server
                    && this.name == that.name
                    && this.owners == that.owners
                    && this.main == that.main
            })
    }

    /// Every login, main first, each additional one with the owners it
    /// reads: names the account set answers belong to. Moving an owner to
    /// another account is a new set, so no answer or hidden repository the
    /// old account recorded is shown for the new one.
    pub fn key(&self) -> String {
        let routes: Vec<String> = self
            .0
            .iter()
            .map(|route| match route.owners.as_slice() {
                [] => route.name.clone(),
                owners => format!("{}({})", route.name, owners.join(" ")),
            })
            .collect();
        routes.join(",")
    }

    /// Every account's login, main first, `login@host` on an Enterprise
    /// Server so the same login on two servers stays two accounts.
    pub fn logins(&self) -> impl Iterator<Item = &str> {
        self.0.iter().map(|route| route.name.as_str())
    }
}

/// One GitHub.com token read as every owner, as before additional accounts.
impl From<Arc<SecretString>> for Tokens {
    fn from(token: Arc<SecretString>) -> Self {
        Self(Arc::new([Route {
            login: String::new(),
            name: String::new(),
            token: ServerToken::github(token),
            owners: Vec::new(),
            main: true,
        }]))
    }
}

#[cfg(test)]
mod tests;
