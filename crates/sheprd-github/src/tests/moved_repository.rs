use crate::{Server, http::moved_to};

const MOVED: &str = "https://api.github.com/repositories/1/dependabot/alerts?state=open";

fn enterprise() -> Server {
    Server::Enterprise("github.example.com".into())
}

#[test]
fn a_moved_repository_continues_at_the_api_address_github_names() {
    for status in [301, 302, 307, 308] {
        assert_eq!(moved_to(&Server::GitHub, status, Some(MOVED)), Some(MOVED));
    }
}

#[test]
fn only_a_redirect_with_a_location_moves_a_read() {
    assert_eq!(moved_to(&Server::GitHub, 200, Some(MOVED)), None);
    assert_eq!(moved_to(&Server::GitHub, 404, Some(MOVED)), None);
    assert_eq!(moved_to(&Server::GitHub, 301, None), None);
}

#[test]
fn a_redirect_never_carries_the_token_off_the_api() {
    for url in [
        "https://example.com/repositories/1",
        "https://api.github.com.example.com/repositories/1",
        "https://api.github.com@example.com/repositories/1",
        "https://api.github.com:8443/repositories/1",
        "http://api.github.com/repositories/1",
        "https://uploads.github.com/repositories/1",
        "/repositories/1",
        "https://api.github.com",
    ] {
        assert_eq!(moved_to(&Server::GitHub, 301, Some(url)), None, "{url}");
    }
}

#[test]
fn an_enterprise_redirect_stays_within_that_servers_api() {
    let moved = "https://github.example.com/api/v3/repositories/1";
    assert_eq!(moved_to(&enterprise(), 301, Some(moved)), Some(moved));
    for url in [
        MOVED,
        "https://github.example.com/repositories/1",
        "https://github.example.com/api/v3.example.com/repositories/1",
        "https://other.example.com/api/v3/repositories/1",
    ] {
        assert_eq!(moved_to(&enterprise(), 301, Some(url)), None, "{url}");
    }
}
