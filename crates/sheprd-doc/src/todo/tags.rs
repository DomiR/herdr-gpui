use super::MAX_TAG;

/// Finds PlainTasks tags in task text. An `@word` inside a code span still
/// counts because this parser intentionally does not parse inline Markdown.
pub fn tags(text: &str) -> Vec<String> {
    let mut found = Vec::new();
    for (at, character) in text.char_indices() {
        if character != '@'
            || (at != 0
                && !text[..at]
                    .chars()
                    .next_back()
                    .is_some_and(char::is_whitespace))
        {
            continue;
        }
        let name = &text[at + 1..];
        let end = name
            .char_indices()
            .take_while(|(_, character)| is_tag_char(*character))
            .last()
            .map_or(0, |(index, character)| index + character.len_utf8());
        if end == 0 || end > MAX_TAG {
            continue;
        }
        let tag = name[..end].to_lowercase();
        if !found.contains(&tag) {
            found.push(tag);
        }
    }
    found
}

/// Normalizes a requested PlainTasks tag.
pub fn tag_name(raw: &str) -> Option<String> {
    let raw = raw.trim();
    let raw = raw.strip_prefix('@').unwrap_or(raw);
    if raw.is_empty() || raw.len() > MAX_TAG || !raw.chars().all(is_tag_char) {
        return None;
    }
    Some(raw.to_lowercase())
}

fn is_tag_char(character: char) -> bool {
    character.is_alphanumeric() || character == '_' || character == '-'
}
