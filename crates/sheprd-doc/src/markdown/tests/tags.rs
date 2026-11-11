use super::*;

#[test]
fn tags_start_at_content_or_after_whitespace() {
    let text = "@start before @after";
    let all = lines(text);
    let line = &all[0];
    assert_eq!(
        tags(text, line)
            .iter()
            .map(|tag| &text[tag.range.clone()])
            .collect::<Vec<_>>(),
        vec!["@start", "@after"]
    );
}

#[test]
fn email_like_text_is_not_a_tag() {
    let text = "a@b.c @valid";
    let all = lines(text);
    let line = &all[0];
    assert_eq!(tags(text, line).len(), 1);
    assert_eq!(&text[tags(text, line)[0].name.clone()], "valid");
}

#[test]
fn parenthesized_and_unclosed_tags_keep_the_expected_range() {
    let text = "@created(26-11-04 10:10) @open(note";
    let all = lines(text);
    let line = &all[0];
    let found = tags(text, line);
    assert_eq!(&text[found[0].range.clone()], "@created(26-11-04 10:10)");
    assert_eq!(&text[found[0].name.clone()], "created");
    assert_eq!(&text[found[1].range.clone()], "@open");
}

#[test]
fn code_spans_and_links_do_not_produce_tags() {
    let text = "`@code` [@link](https://example.test) @valid";
    let all = lines(text);
    let line = &all[0];
    let found = tags(text, line);
    assert_eq!(found.len(), 1);
    assert_eq!(&text[found[0].range.clone()], "@valid");
}

#[test]
fn tags_are_allowed_on_headings_and_tasks() {
    let text = "# @heading\n- [x] done @done\n- [-] cancelled @cancelled";
    let all = lines(text);
    assert_eq!(&text[tags(text, &all[0])[0].name.clone()], "heading");
    assert_eq!(&text[tags(text, &all[1])[0].name.clone()], "done");
    assert_eq!(&text[tags(text, &all[2])[0].name.clone()], "cancelled");
}

#[test]
fn named_kinds_are_case_insensitive() {
    let text = "@TODAY @High @critical @LOW @done @lasted @cancelled @DONE @due";
    let all = lines(text);
    let kinds = tags(text, &all[0])
        .into_iter()
        .map(|tag| tag.kind)
        .collect::<Vec<_>>();
    assert_eq!(
        kinds,
        vec![
            TagKind::Today,
            TagKind::High,
            TagKind::Critical,
            TagKind::Low,
            TagKind::Done,
            TagKind::Done,
            TagKind::Cancelled,
            TagKind::Done,
            TagKind::Generic,
        ]
    );
}

#[test]
fn fenced_code_and_code_lines_have_no_tags() {
    let text = "```\n@fenced\n```\n@paragraph";
    let all = lines(text);
    assert!(tags(text, &all[0]).is_empty());
    assert!(tags(text, &all[1]).is_empty());
    assert!(tags(text, &all[2]).is_empty());
    assert_eq!(tags(text, &all[3]).len(), 1);
}

#[test]
fn stamp_tags_take_a_space_before_their_date() {
    let text = "- [x] ship @done (26-11-04 10:10) @high (not a date)";
    let all = lines(text);
    let found = tags(text, &all[0]);
    let texts: Vec<_> = found.iter().map(|tag| &text[tag.range.clone()]).collect();
    assert_eq!(texts, vec!["@done (26-11-04 10:10)", "@high"]);
}
