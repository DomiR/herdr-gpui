//! Opening a pull request for the checkout's branch: the push, the
//! repository lookup, and the mutation, sent by the account that owns the
//! head on the base repository's server.

use super::{API_TIMEOUT, CREATE_MUTATION, Error, Outcome, REPOSITORY_QUERY, TITLE_LIMIT, git};
use sheprd_github::{
    Tokens,
    pull_request::{base_repository, clean},
};
use std::time::Instant;

pub(super) fn create_pull_request(
    checkout: &str,
    branch: &str,
    tokens: &Tokens,
    deadline: Instant,
    cancelled: &impl Fn() -> bool,
) -> crate::Result<Outcome> {
    // A fork checkout opens its pull request against `upstream`, from the
    // branch pushed to its own `origin` below. The fork's owner opens it, so
    // that owner's account sends the mutation.
    let base = base_repository(checkout, deadline, cancelled)?;
    let head = base.head(branch)?;
    let token = tokens
        .for_owner(
            &base.server,
            base.fork_owner.as_deref().unwrap_or(&base.owner),
        )
        .ok_or(sheprd_github::Error::from(
            sheprd_github::pull_request::Error::Origin,
        ))?;
    let title = clean(&git(
        checkout,
        &["log", "-1", "--pretty=format:%s"],
        "read the commit subject",
        deadline,
        cancelled,
    )?);
    let title: String = title.trim().chars().take(TITLE_LIMIT).collect();
    if title.is_empty() {
        return Err(Error::PullRequestTitle.into());
    }
    // The head ref must exist on GitHub before a pull request can reference it.
    git(
        checkout,
        &["push", "--set-upstream", "origin", branch],
        "push",
        deadline,
        cancelled,
    )?;
    let mut cooldown = None;
    let repository = sheprd_github::graphql(
        "create_pr_repository",
        token,
        REPOSITORY_QUERY,
        serde_json::json!({"owner": base.owner, "repo": base.repo}),
        API_TIMEOUT,
        cancelled,
        &mut cooldown,
    )?;
    let id = repository["data"]["repository"]["id"]
        .as_str()
        .filter(|id| !id.is_empty())
        .ok_or(sheprd_github::Error::from(
            sheprd_github::pull_request::Error::Repository,
        ))?
        .to_owned();
    let default_branch = repository["data"]["repository"]["defaultBranchRef"]["name"]
        .as_str()
        .filter(|name| !name.is_empty())
        .ok_or(sheprd_github::Error::from(
            sheprd_github::pull_request::Error::Repository,
        ))?
        .to_owned();
    if base.is_own_base(branch, &default_branch) {
        return Err(Error::PullRequestBase.into());
    }
    let created = sheprd_github::graphql(
        "create_pr",
        token,
        CREATE_MUTATION,
        serde_json::json!({
            "repository": id, "base": default_branch, "head": head, "title": title, "body": ""
        }),
        API_TIMEOUT,
        cancelled,
        &mut cooldown,
    )?;
    parse_created(&created, &token.server, &base.owner, &base.repo)
}

pub(super) fn parse_created(
    response: &serde_json::Value,
    server: &sheprd_github::Server,
    owner: &str,
    repo: &str,
) -> crate::Result<Outcome> {
    let pr = &response["data"]["createPullRequest"]["pullRequest"];
    let number = pr["number"].as_u64().filter(|number| *number > 0);
    let url = pr["url"].as_str().unwrap_or_default();
    let expected =
        number.map(|number| format!("{}/{owner}/{repo}/pull/{number}", server.web_url()));
    if expected.as_deref() != Some(url) {
        return Err(
            sheprd_github::Error::from(sheprd_github::pull_request::Error::Identity).into(),
        );
    }
    Ok(Outcome {
        message: format!("Opened pull request #{}", number.unwrap_or_default()),
        url: expected,
        restacked: None,
    })
}
