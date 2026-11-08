//! PlainTasks-compatible stamps in task labels.

use chrono::{NaiveDateTime, Timelike};
use std::ops::Range;

/// A supported PlainTasks stamp tag.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tag {
    Started,
    Done,
    Cancelled,
    Lasted,
}

impl Tag {
    /// The tag name without its @ prefix.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Started => "started",
            Self::Done => "done",
            Self::Cancelled => "cancelled",
            Self::Lasted => "lasted",
        }
    }
}

/// The fixed-width local date format used in stamps.
pub const DATE_FORMAT: &str = "%y-%m-%d %H:%M";

const MAX_DAYS: u64 = 999_999_999;

/// Formats a stamp, including its tag-specific spacing.
pub fn stamp(tag: Tag, at: NaiveDateTime) -> String {
    let space = if tag == Tag::Started { "" } else { " " };
    format!("@{}{}({})", tag.name(), space, at.format(DATE_FORMAT))
}

/// Formats the wall-clock span between two minute-precision readings.
pub fn lasted(from: NaiveDateTime, to: NaiveDateTime) -> String {
    let total = match (wall_minute(from), wall_minute(to)) {
        (Some(from), Some(to)) => to.signed_duration_since(from).num_minutes().max(0) as u64,
        _ => 0,
    }
    .min(MAX_DAYS * 24 * 60 + 24 * 60 - 1);
    let days = total / (24 * 60);
    let hours = (total % (24 * 60)) / 60;
    let minutes = total % 60;
    let time = format!("{hours}:{minutes:02}");
    if days == 0 {
        return time;
    }
    let day = if days == 1 { "day" } else { "days" };
    if hours == 0 && minutes == 0 {
        format!("{days} {day}")
    } else {
        format!("{days} {day}, {time}")
    }
}

pub(super) fn lasted_stamp(value: &str) -> String {
    format!("@lasted({value})")
}

fn wall_minute(at: NaiveDateTime) -> Option<NaiveDateTime> {
    at.date().and_hms_opt(at.hour(), at.minute(), 0)
}

struct FoundStamp {
    tag: Tag,
    range: Range<usize>,
    value: Range<usize>,
}

fn scan(label: &str) -> Vec<FoundStamp> {
    let bytes = label.as_bytes();
    let mut found = Vec::new();
    let mut at = 0;
    while at < bytes.len() {
        if bytes[at] != b'@' || (at > 0 && !is_blank(bytes[at - 1])) {
            at += 1;
            continue;
        }
        let Some(tag) = tag_at(bytes, at) else {
            at += 1;
            continue;
        };
        let mut open = at + 1 + tag.name().len();
        while open < bytes.len() && is_blank(bytes[open]) {
            open += 1;
        }
        if bytes.get(open) != Some(&b'(') {
            at += 1;
            continue;
        }
        let value_start = open + 1;
        let mut close = value_start;
        while close < bytes.len()
            && bytes[close] != b'('
            && bytes[close] != b')'
            && bytes[close] != b'\r'
            && bytes[close] != b'\n'
        {
            close += 1;
        }
        if bytes.get(close) != Some(&b')') {
            at += 1;
            continue;
        }
        let mut start = at;
        while start > 0 && is_blank(bytes[start - 1]) {
            start -= 1;
        }
        found.push(FoundStamp {
            tag,
            range: start..close + 1,
            value: value_start..close,
        });
        at = close + 1;
    }
    found
}

fn tag_at(bytes: &[u8], at: usize) -> Option<Tag> {
    [Tag::Started, Tag::Done, Tag::Cancelled, Tag::Lasted]
        .into_iter()
        .find(|tag| {
            let start = at + 1;
            let end = start + tag.name().len();
            bytes.get(start..end) == Some(tag.name().as_bytes())
                && bytes
                    .get(end)
                    .is_none_or(|byte| !byte.is_ascii_alphanumeric() && *byte != b'_')
        })
}

fn is_blank(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t')
}

/// The first dated @started stamp in label.
pub fn started_at(label: &str) -> Option<NaiveDateTime> {
    scan(label).into_iter().find_map(|found| {
        (found.tag == Tag::Started)
            .then(|| parse_date(&label[found.value]))
            .flatten()
    })
}

fn parse_date(value: &str) -> Option<NaiveDateTime> {
    (value.len() == 14)
        .then(|| NaiveDateTime::parse_from_str(value, DATE_FORMAT).ok())
        .flatten()
}

/// The byte ranges of stamps with tag, including their preceding blanks.
pub fn spans(label: &str, tag: Tag) -> Vec<Range<usize>> {
    scan(label)
        .into_iter()
        .filter(|found| found.tag == tag)
        .map(|found| found.range)
        .collect()
}

/// Removes all recognized stamps and trims the remaining label.
pub fn without_stamps(label: &str) -> String {
    let ranges: Vec<_> = scan(label).into_iter().map(|found| found.range).collect();
    remove_ranges(label, &ranges).trim().to_owned()
}

fn remove_ranges(text: &str, ranges: &[Range<usize>]) -> String {
    let mut out = String::with_capacity(text.len());
    let mut at = 0;
    for range in ranges {
        out.push_str(&text[at..range.start]);
        at = range.end;
    }
    out.push_str(&text[at..]);
    out
}
