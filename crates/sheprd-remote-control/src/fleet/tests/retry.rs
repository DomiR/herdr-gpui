//! A grant write that failed is tried again with no fleet change.

use super::*;

#[tokio::test(flavor = "multi_thread")]
async fn a_failed_grant_write_is_retried() {
    let (http, _, _) = relay().await;
    let (a, mut b) = (Node::new(http, 1).await, Node::new(http, 2).await);
    assert_eq!(a.ask(Ask::Found), Answer::Done);
    join(&a, &b);
    // b's `authorized_keys` cannot be written: a directory stands there.
    let path = b.dir.path().join("authorized_keys");
    std::fs::create_dir(&path).unwrap();
    std::fs::write(path.join("blocker"), b"").unwrap();
    b.start().await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert!(b.lines().is_empty());
    // Freed, with no fleet change: the next try writes a's line.
    std::fs::remove_dir_all(&path).unwrap();
    until(10, || b.lines().len() == 1).await;
}
