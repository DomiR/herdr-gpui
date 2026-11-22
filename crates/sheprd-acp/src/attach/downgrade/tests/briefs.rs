use super::*;

#[test]
fn a_client_before_briefs_gets_the_count_in_their_place() {
    let brief = Change::Brief {
        brief: sheprd_chat_protocol::Brief {
            entry: sheprd_chat_protocol::EntryId(3),
            from: None,
        },
    };
    let epoch = Cursor::new_epoch();
    let mut revision_three = Downgrade::new(3);
    assert!(revision_three.message(snapshot(2, epoch)).is_some());
    assert_eq!(
        revision_three.message(change(brief.clone(), 1, epoch)),
        Some(change(Change::Clients { count: 2 }, 1, epoch))
    );
    let mut current = Downgrade::new(REVISION);
    let message = change(brief, 1, epoch);
    assert_eq!(current.message(message.clone()), Some(message));
}
