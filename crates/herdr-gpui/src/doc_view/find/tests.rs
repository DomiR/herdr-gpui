use super::*;

#[test]
fn finds_ascii_case_insensitively_by_default() {
    assert_eq!(
        find_all("Foo foo", "foo", false, MAX_MATCHES).0,
        vec![0..3, 4..7]
    );
    assert_eq!(find_all("Foo foo", "foo", true, MAX_MATCHES).0, vec![4..7]);
}

#[test]
fn keeps_utf8_offsets_at_character_boundaries() {
    assert_eq!(
        find_all("Ä foo ä FOO", "foo", false, MAX_MATCHES).0,
        vec![3..6, 10..13]
    );
    assert_eq!(find_all("ÄÄ", "Ä", true, MAX_MATCHES).0, vec![0..2, 2..4]);
}

#[test]
fn matches_do_not_overlap() {
    assert_eq!(find_all("aaa", "aa", true, MAX_MATCHES).0, vec![0..2]);
}

#[test]
fn an_empty_query_has_no_matches() {
    assert_eq!(find_all("anything", "", false, MAX_MATCHES), (Vec::new(), false));
}

#[test]
fn reports_when_the_match_cap_is_reached() {
    let (matches, capped) = find_all("aaaa", "a", true, 2);
    assert_eq!(matches, vec![0..1, 1..2]);
    assert!(capped);
}

#[test]
fn caps_the_query_on_a_character_boundary() {
    let text = "a".repeat(MAX_QUERY + 1);
    let query = "a".repeat(MAX_QUERY + 1);
    let (matches, capped) = find_all(&text, &query, true, MAX_MATCHES);
    assert_eq!(matches, vec![0..MAX_QUERY]);
    assert!(!capped);
}
