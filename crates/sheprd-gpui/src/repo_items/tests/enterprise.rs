//! An Enterprise repository listed through an SSH remote links to the HTTPS
//! port of the account that reads it.

use super::*;
use crate::pull_request::{Base, Remote};
use sheprd_github::Server;

#[test]
fn links_use_the_port_of_the_reading_account() {
    let base = Base {
        server: Server::enterprise("ghe.example.com").unwrap(),
        owner: "sheprd-labs".into(),
        repo: "sheprd".into(),
        remote: Remote::Origin,
        fork_owner: None,
    };
    let server = Server::enterprise("ghe.example.com:8443").unwrap();
    let origin = Origin::read_on(base, server);
    let items = parse(&response(), &origin, Kind::PullRequest).unwrap();
    assert!(!items.is_empty());
    for item in items {
        assert!(
            item.url
                .starts_with("https://ghe.example.com:8443/sheprd-labs/sheprd/"),
            "{}",
            item.url
        );
    }
}
