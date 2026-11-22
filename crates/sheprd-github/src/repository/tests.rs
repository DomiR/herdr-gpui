#![allow(clippy::unwrap_used)]
use super::*;

fn repo(server: Server, owner: &str, name: &str) -> Option<Repo> {
    Some(Repo {
        server,
        owner: owner.into(),
        name: name.into(),
    })
}

fn ghe(host: &str) -> Server {
    Server::Enterprise(host.into())
}

#[test]
fn github_com_remotes_name_github_com() {
    for remote in [
        "git@github.com:Owner/project.git",
        "ssh://git@github.com/owner/project",
        "https://github.com/owner/project.git",
        "https://user:token@github.com/owner/project",
    ] {
        assert_eq!(
            github_repo(remote),
            repo(Server::GitHub, "owner", "project"),
            "{remote}"
        );
    }
}

#[test]
fn github_com_is_named_by_its_bare_host_and_default_ssh_port_only() {
    for remote in [
        "https://github.com:443/owner/project",
        "https://www.github.com/owner/project",
        "https://api.github.com/owner/project",
        "ssh://git@github.com:22/owner/project",
        "git@www.github.com:owner/project",
    ] {
        assert_eq!(github_repo(remote), None, "{remote}");
    }
}

#[test]
fn any_other_host_names_an_enterprise_server() {
    let server = ghe("ghe.example.com");
    for remote in [
        "git@GHE.example.com:owner/project.git",
        "ssh://git@ghe.example.com/owner/project",
        "https://ghe.example.com/owner/project",
        "https://ghe.example.com:443/owner/project",
    ] {
        assert_eq!(
            github_repo(remote),
            repo(server.clone(), "owner", "project"),
            "{remote}"
        );
    }
    assert_eq!(
        github_repo("https://ghe.example.com:8443/owner/project"),
        repo(ghe("ghe.example.com:8443"), "owner", "project")
    );
}

#[test]
fn an_ssh_port_is_not_the_servers_https_port() {
    assert_eq!(
        github_repo("ssh://git@ghe.example.com:2222/owner/project"),
        repo(ghe("ghe.example.com"), "owner", "project")
    );
}

#[test]
fn malformed_hosts_name_no_repository() {
    for remote in [
        "git@ghe.example.com:22:owner/project",
        "https://ghe_example.com/owner/project",
        "https://ghe.example.com:0/owner/project",
        "https://@ghe.example.com/owner/project",
        "https://a@b@ghe.example.com/owner/project",
        "https://ghe.example.com/owner",
    ] {
        assert_eq!(github_repo(remote), None, "{remote}");
    }
}

#[test]
fn the_same_repository_is_named_across_case_and_remote_kinds() {
    let ssh = github_repo("git@ghe.example.com:owner/project").unwrap();
    let https = github_repo("https://ghe.example.com:8443/Owner/Project").unwrap();
    assert!(ssh.same(&https));
    let github = github_repo("git@github.com:owner/project").unwrap();
    assert!(!ssh.same(&github));
}
