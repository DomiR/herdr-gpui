use super::*;

#[test]
fn an_enterprise_token_is_saved_for_its_server() {
    let id = "0123456789abcdef0123456789abcdef";
    let server = Server::enterprise("ghe.example.com:8443").unwrap();
    let account = Account::enterprise(&server, id).unwrap();
    assert_eq!(
        account.file_name().to_str().unwrap(),
        format!("github-credentials-enterprise-ghe.example.com_8443-{id}")
    );
    // An account moved to another server in the config never reads the old token.
    let moved = Account::enterprise(&Server::enterprise("ghe.example.com").unwrap(), id).unwrap();
    assert_ne!(account.file_name(), moved.file_name());
}
