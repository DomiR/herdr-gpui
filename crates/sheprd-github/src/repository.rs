//! GitHub repositories named by Git remote URLs, on GitHub.com or on a GitHub
//! Enterprise Server host. Naming a repository sends nothing: only a server
//! some account signed in to is ever asked about it.

use crate::Server;

/// A bound on each remote path segment, above GitHub's own name limits.
const MAX_SEGMENT: usize = 100;

/// A repository and the server its remote names. The owner is lowercased.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Repo {
    pub server: Server,
    pub owner: String,
    pub name: String,
}

impl Repo {
    /// Whether both name the same repository: owner and repository names
    /// are case-insensitive on every GitHub server.
    pub fn same(&self, other: &Self) -> bool {
        self.server.same(&other.server)
            && self.owner.eq_ignore_ascii_case(&other.owner)
            && self.name.eq_ignore_ascii_case(&other.name)
    }
}

/// One segment of a remote's path.
///
/// Account and repository names are GitHub's to define, so no name grammar is
/// reproduced here: a segment is refused only when it names the directory
/// itself or carries punctuation that would not stay inside its own path
/// segment in the avatar URL built from it.
fn segment(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_SEGMENT
        && value != "."
        && value != ".."
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
}

/// The owner and repository of GitHub's `nameWithOwner`, held to the same
/// segments as a remote's, since a pull request URL is built from them.
pub(crate) fn name_with_owner(value: &str) -> Option<(&str, &str)> {
    value
        .split_once('/')
        .filter(|(owner, repo)| segment(owner) && segment(repo))
}

/// The `owner/repo` repository a remote URL names, and the server it is on.
pub fn github_repo(remote: &str) -> Option<Repo> {
    let (server, path) = ssh_path(remote).or_else(|| http_path(remote))?;
    let (owner, repo) = path.split_once('/')?;
    let repo = repo.strip_suffix(".git").unwrap_or(repo);
    if !segment(owner) || !segment(repo) {
        return None;
    }
    Some(Repo {
        server,
        owner: owner.to_ascii_lowercase(),
        name: repo.to_owned(),
    })
}

/// The server and path of `git@host:path` or `ssh://git@host[:port]/path`.
/// An SSH port is not the server's HTTPS port, so it is dropped. GitHub.com
/// takes SSH on its default port only.
fn ssh_path(remote: &str) -> Option<(Server, &str)> {
    let (host, port, path) = match remote.strip_prefix("ssh://git@") {
        Some(rest) => {
            let (authority, path) = rest.split_once('/')?;
            match authority.split_once(':') {
                Some((host, _)) => (host, true, path),
                None => (authority, false, path),
            }
        }
        None => {
            let (host, path) = remote.strip_prefix("git@")?.split_once(':')?;
            (host, false, path)
        }
    };
    if host.contains(':') {
        return None;
    }
    let server = Server::of_host(host)?;
    (!port || server.is_enterprise()).then_some((server, path))
}

/// The server and path of an `http(s)://[userinfo@]host[:port]/` remote.
///
/// Git accepts a user name, or a user name and token, before the host
/// (`https://user@github.com/o/r`, `https://user:token@github.com/o/r`).
/// The host is what Git connects to, so it alone names the server; the
/// userinfo is skipped and never kept. Userinfo that a URL parser could read
/// as the end of the authority (`/`, `?`, `#`, `\`, a second `@`) is refused
/// so the host cannot be misread. GitHub.com is named by its bare host only.
fn http_path(remote: &str) -> Option<(Server, &str)> {
    let rest = ["https://", "http://"]
        .into_iter()
        .find_map(|scheme| remote.strip_prefix(scheme))?;
    let (authority, path) = rest.split_once('/')?;
    let host = match authority.split_once('@') {
        None => authority,
        Some((userinfo, host)) => {
            let plain = |b: u8| b.is_ascii_graphic() && !matches!(b, b'@' | b'?' | b'#' | b'\\');
            if userinfo.is_empty() || !userinfo.bytes().all(plain) {
                return None;
            }
            host
        }
    };
    let server = Server::of_host(host)?;
    (server.is_enterprise() || host == "github.com").then_some((server, path))
}

#[cfg(test)]
mod tests;
