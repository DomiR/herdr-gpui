use super::*;

fn enterprise(host: &str) -> Option<Server> {
    Some(Server::Enterprise(host.into()))
}

#[test]
fn github_com_is_named_by_its_bare_host_or_the_https_port() {
    for host in ["github.com", "GitHub.com", "github.com:443"] {
        assert_eq!(Server::of_host(host), Some(Server::GitHub), "{host}");
    }
}

#[test]
fn github_coms_other_hosts_and_ports_name_no_server() {
    for host in [
        "www.github.com",
        "api.github.com",
        "github.com:8443",
        "api.github.com:443",
    ] {
        assert_eq!(Server::of_host(host), None, "{host}");
    }
}

#[test]
fn any_other_valid_host_is_an_enterprise_server_lowercased() {
    assert_eq!(
        Server::of_host("GitHub.Example.com"),
        enterprise("github.example.com")
    );
    assert_eq!(
        Server::of_host("github.example.com:443"),
        enterprise("github.example.com")
    );
    assert_eq!(
        Server::of_host("github.example.com:8443"),
        enterprise("github.example.com:8443")
    );
    assert_eq!(Server::of_host("ghe"), enterprise("ghe"));
}

#[test]
fn malformed_hosts_and_ports_name_no_server() {
    let long_label = format!("{}.example.com", "a".repeat(64));
    let long_host = format!("{}.com", "a.".repeat(140));
    for host in [
        "",
        ".example.com",
        "example..com",
        "-ghe.example.com",
        "ghe-.example.com",
        "ghe_example.com",
        "ghe.example.com/path",
        "user@ghe.example.com",
        "ghe.example.com:",
        "ghe.example.com:0",
        "ghe.example.com:08443",
        "ghe.example.com:65536",
        "ghe.example.com:1:2",
        long_label.as_str(),
        long_host.as_str(),
    ] {
        assert_eq!(Server::of_host(host), None, "{host}");
    }
}

#[test]
fn api_addresses_follow_the_server() {
    assert_eq!(
        Server::GitHub.graphql_url(),
        "https://api.github.com/graphql"
    );
    assert_eq!(Server::GitHub.rest_root(), "https://api.github.com");
    assert_eq!(Server::GitHub.web_url(), "https://github.com");
    let server = Server::Enterprise("ghe.example.com:8443".into());
    assert_eq!(
        server.graphql_url(),
        "https://ghe.example.com:8443/api/graphql"
    );
    assert_eq!(server.rest_root(), "https://ghe.example.com:8443/api/v3");
    assert_eq!(server.web_url(), "https://ghe.example.com:8443");
}

#[test]
fn an_ssh_remote_names_a_server_by_host_name_alone() {
    let ported = Server::Enterprise("ghe.example.com:8443".into());
    let ssh = Server::Enterprise("ghe.example.com".into());
    assert!(ported.serves(&ssh));
    assert!(ported.serves(&ported));
    assert!(!ssh.serves(&ported));
    assert!(!ported.serves(&Server::Enterprise("ghe.example.com:9443".into())));
    assert!(!Server::GitHub.serves(&ssh));
    assert!(!ssh.serves(&Server::GitHub));
}
