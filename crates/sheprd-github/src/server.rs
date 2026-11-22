//! The GitHub server a repository lives on and an account signs in to:
//! GitHub.com, or a GitHub Enterprise Server host. A token is sent only to
//! the API of the server its account signed in to.

use std::{fmt, sync::Arc};

/// A bound above the longest DNS name, with room for a port.
const MAX_HOST: usize = 260;

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub enum Server {
    #[default]
    GitHub,
    /// A GitHub Enterprise Server host name, lowercased, with an HTTPS port
    /// when it is not 443.
    Enterprise(Arc<str>),
}

impl Server {
    /// The server at `host`, a host name with an optional HTTPS port and no
    /// scheme or path: GitHub.com for `github.com`, any other valid host an
    /// Enterprise Server. GitHub.com's other hosts name no server.
    pub fn of_host(host: &str) -> Option<Self> {
        let host = host.to_ascii_lowercase();
        let (name, port) = match host.split_once(':') {
            Some((name, port)) => (name, Some(port)),
            None => (host.as_str(), None),
        };
        let label = |label: &str| {
            !label.is_empty()
                && label.len() <= 63
                && !label.starts_with('-')
                && !label.ends_with('-')
                && label
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-')
        };
        let port_ok = port.is_none_or(|port| {
            !port.starts_with('0') && port.parse::<u16>().is_ok_and(|port| port != 0)
        });
        if host.len() > MAX_HOST || !name.split('.').all(label) || !port_ok {
            return None;
        }
        match (name, port) {
            ("github.com", None | Some("443")) => Some(Self::GitHub),
            ("github.com" | "www.github.com" | "api.github.com", _) => None,
            (name, None | Some("443")) => Some(Self::Enterprise(name.into())),
            _ => Some(Self::Enterprise(host.into())),
        }
    }

    /// An Enterprise Server as an account's config names it, by
    /// [`Server::of_host`]'s rules. GitHub.com is refused, because its
    /// accounts sign in on their own.
    pub fn enterprise(host: &str) -> Option<Self> {
        Self::of_host(host).filter(Self::is_enterprise)
    }

    /// The host and HTTPS port the server's web pages and API are on.
    pub fn host(&self) -> &str {
        match self {
            Self::GitHub => "github.com",
            Self::Enterprise(host) => host,
        }
    }

    /// The host name alone, which is all an SSH remote can name: its port is
    /// the SSH port, never the server's HTTPS port.
    fn name(&self) -> &str {
        self.host().split(':').next().unwrap_or_default()
    }

    /// Whether `remote`, the server a remote URL names, is this server. An
    /// SSH remote names no HTTPS port, so its host name alone must match.
    pub fn serves(&self, remote: &Self) -> bool {
        self == remote || (!remote.host().contains(':') && self.name() == remote.host())
    }

    /// Whether two remotes' servers are one, either naming no HTTPS port.
    pub(crate) fn same(&self, other: &Self) -> bool {
        self.serves(other) || other.serves(self)
    }

    pub fn is_enterprise(&self) -> bool {
        matches!(self, Self::Enterprise(_))
    }

    /// The server's web address, such as `https://github.com`.
    pub fn web_url(&self) -> String {
        format!("https://{}", self.host())
    }

    pub(crate) fn graphql_url(&self) -> String {
        match self {
            Self::GitHub => "https://api.github.com/graphql".to_owned(),
            Self::Enterprise(host) => format!("https://{host}/api/graphql"),
        }
    }

    /// The name an account on this server goes by in logs, pauses, and
    /// caches: its login on GitHub.com, `login@host` elsewhere.
    pub fn account_name(&self, login: &str) -> String {
        match self {
            Self::GitHub => login.to_owned(),
            Self::Enterprise(host) => format!("{login}@{host}"),
        }
    }

    /// The root REST paths are appended to, without a trailing slash.
    pub(crate) fn rest_root(&self) -> String {
        match self {
            Self::GitHub => "https://api.github.com".to_owned(),
            Self::Enterprise(host) => format!("https://{host}/api/v3"),
        }
    }
}

impl fmt::Display for Server {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.host())
    }
}

#[cfg(test)]
mod tests;
