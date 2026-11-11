use percent_encoding::percent_decode_str;

use super::MAX_PATH;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Reference {
    pub(super) path: String,
    pub(super) home: bool,
    pub(super) line: Option<u32>,
}

/// Parses one reference and removes its optional location suffix.
pub(super) fn parse(text: &str, decode: bool) -> Option<Reference> {
    let (text, angled) = match text.strip_prefix('<') {
        Some(text) if text.ends_with('>') => (&text[..text.len() - 1], true),
        Some(_) => return None,
        None => (text, false),
    };
    if text.is_empty() || text.len() > MAX_PATH {
        return None;
    }
    if !angled && text.chars().any(|ch| ch.is_whitespace() || ch.is_control()) {
        return None;
    }

    let file_url = text.strip_prefix("file://");
    let mut path = if let Some(mut path) = file_url {
        if path == "localhost" {
            path = "";
        } else if path.starts_with("localhost/") {
            path = &path["localhost".len()..];
        } else if !path.starts_with('/') {
            return None;
        }
        #[cfg(windows)]
        if let Some(rest) = path.strip_prefix('/')
            && rest.as_bytes().get(1) == Some(&b':')
        {
            path = rest;
        }
        decode_path(path)?
    } else {
        if text.contains("://") || has_non_drive_scheme(text) {
            return None;
        }
        if decode {
            decode_path(text)?
        } else {
            text.to_owned()
        }
    };
    if path.is_empty() || path.starts_with('-') {
        return None;
    }
    if path.chars().any(char::is_control) {
        return None;
    }

    let line = line_suffix(&mut path);
    if path.is_empty() || path.starts_with('#') || path.starts_with('-') {
        return None;
    }
    let home = path.starts_with("~/");
    if home {
        path = path[2..].to_owned();
    }
    (!path.is_empty()).then_some(Reference { path, home, line })
}

fn decode_path(path: &str) -> Option<String> {
    let decoded = percent_decode_str(path).decode_utf8().ok()?.into_owned();
    (decoded.len() <= MAX_PATH).then_some(decoded)
}

fn has_non_drive_scheme(text: &str) -> bool {
    let Some(colon) = text.find(':') else {
        return false;
    };
    let scheme = &text[..colon];
    let suffix = &text[colon + 1..];
    if scheme.len() == 1 && scheme.as_bytes()[0].is_ascii_alphabetic() {
        return false;
    }
    let location = |text: &str| !text.is_empty() && text.bytes().all(|byte| byte.is_ascii_digit());
    let numeric_location = suffix
        .split_once(':')
        .map_or(location(suffix), |(line, column)| {
            location(line) && location(column)
        });
    if numeric_location {
        return false;
    }
    !scheme.is_empty() && scheme.bytes().all(|byte| byte.is_ascii_alphabetic())
}

fn line_suffix(path: &mut String) -> Option<u32> {
    let hash = path.rfind("#L");
    if let Some(start) = hash
        && let Some(line) = parse_hash_suffix(&path[start + 2..])
    {
        path.truncate(start);
        return Some(line);
    }

    let last_colon = path.rfind(':')?;
    let tail = &path[last_colon + 1..];
    if tail.is_empty() || !tail.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let (line_start, line_text) = if let Some(previous) = path[..last_colon].rfind(':') {
        let possible_line = &path[previous + 1..last_colon];
        if !possible_line.is_empty() && possible_line.bytes().all(|byte| byte.is_ascii_digit()) {
            (previous, possible_line)
        } else {
            (last_colon, tail)
        }
    } else {
        (last_colon, tail)
    };
    let line = line_text.parse::<u32>().ok()?;
    if line == 0 {
        return None;
    }
    path.truncate(line_start);
    Some(line)
}

fn parse_hash_suffix(suffix: &str) -> Option<u32> {
    let (first, last) = suffix
        .split_once('-')
        .map_or((suffix, None), |(first, last)| (first, Some(last)));
    if last.is_some_and(|last| last.contains('-')) {
        return None;
    }
    let line = location_line(first, false)?;
    if let Some(last) = last {
        location_line(last, true)?;
    }
    Some(line)
}

fn location_line(text: &str, leading_l: bool) -> Option<u32> {
    let text = if leading_l {
        text.strip_prefix('L').unwrap_or(text)
    } else {
        text
    };
    let line = text.split_once('C').map_or(text, |(line, column)| {
        if !column.is_empty() && column.bytes().all(|byte| byte.is_ascii_digit()) {
            line
        } else {
            ""
        }
    });
    if !line.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let line = line.parse::<u32>().ok()?;
    (line > 0).then_some(line)
}
