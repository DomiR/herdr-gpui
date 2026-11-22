//! Short text both clients show the same way: durations, token counts,
//! costs, how a turn ended, and pasted text without its envelope.

use crate::{Cost, TurnEnd, TurnSummary};

/// `950ms`, `12s`, `3m 4s`, or `1h 2m`.
pub fn duration(ms: u64) -> String {
    let seconds = ms / 1000;
    match seconds {
        0 => format!("{ms}ms"),
        1..60 => format!("{seconds}s"),
        60..3600 => format!("{}m {}s", seconds / 60, seconds % 60),
        _ => format!("{}h {}m", seconds / 3600, seconds % 3600 / 60),
    }
}

/// `950`, `18.6k`, or `1.2M` tokens.
pub fn tokens(count: u64) -> String {
    match count {
        0..1000 => count.to_string(),
        1000..1_000_000 => trim(format!("{:.1}", count as f64 / 1000.0)) + "k",
        _ => trim(format!("{:.1}", count as f64 / 1_000_000.0)) + "M",
    }
}

fn trim(number: String) -> String {
    number
        .strip_suffix(".0")
        .map_or(number.clone(), str::to_owned)
}

/// `$0.42`, or `0.42 EUR` for a currency other than dollars.
pub fn cost(cost: &Cost) -> String {
    let amount = cost.micros as f64 / 1_000_000.0;
    let digits = if amount < 0.01 && cost.micros > 0 {
        4
    } else {
        2
    };
    match cost.currency.as_str() {
        "USD" | "usd" | "" => format!("${amount:.digits$}"),
        currency => format!("{amount:.digits$} {currency}"),
    }
}

impl TurnSummary {
    /// `Worked 12s · 3.2k in · 450 out`.
    pub fn describe(&self) -> String {
        let verb = match self.end {
            TurnEnd::Done => "Worked",
            TurnEnd::Cancelled => "Cancelled after",
            TurnEnd::Limit => "Stopped at its limit after",
            TurnEnd::Refused => "Refused after",
            TurnEnd::Failed => "Failed after",
            TurnEnd::Other => "Ended after",
        };
        let mut text = format!("{verb} {}", duration(self.duration_ms));
        if let Some(tokens) = &self.tokens {
            text.push_str(&format!(
                " \u{b7} {} in \u{b7} {} out",
                self::tokens(tokens.input),
                self::tokens(tokens.output)
            ));
            if tokens.thought > 0 {
                text.push_str(&format!(
                    " \u{b7} {} thinking",
                    self::tokens(tokens.thought)
                ));
            }
        }
        text
    }
}

/// Agents such as Claude wrap pasted text in a `<pasted_content id="…">`
/// envelope, which reaches logs, titles, and replayed prompts. Shown, the
/// text is what was pasted: `text` without a leading envelope, `text` itself
/// when it has none, and `None` when nothing is left (blank, or only the
/// envelope's tags). Ordinary markup inside the text is never touched.
pub fn unwrap_paste(text: &str) -> Option<&str> {
    const OPEN: &str = "<pasted_content";
    const CLOSE: &str = "</pasted_content>";
    let original = text;
    let text = text.trim();
    if text.is_empty() || text == CLOSE {
        return None;
    }
    let Some(rest) = text.strip_prefix(OPEN) else {
        return Some(original);
    };
    // Only the exact tag Claude writes is unwrapped. Anything more in it,
    // such as other attributes, would reach the agent unseen, so such text
    // shows as it is.
    let Some(body) = rest.strip_prefix('>').or_else(|| paste_id(rest)) else {
        // A title cut off inside the tag is a marker with nothing to show.
        let cut = rest
            .strip_prefix(" id=\"")
            .is_some_and(|id| id.len() <= MAX_PASTE_ID && id.bytes().all(id_byte));
        return (!cut).then_some(original);
    };
    let body = body.trim();
    let body = body.strip_suffix(CLOSE).unwrap_or(body).trim();
    (!body.is_empty()).then_some(body)
}

/// The text after a ` id="ID">` opening tag's end, where the id is a short
/// run of letters, digits, `-` and `_`.
fn paste_id(rest: &str) -> Option<&str> {
    let rest = rest.strip_prefix(" id=\"")?;
    let (id, rest) = rest.split_once("\">")?;
    ((1..=MAX_PASTE_ID).contains(&id.len()) && id.bytes().all(id_byte)).then_some(rest)
}

/// The longest paste envelope id unwrapped.
const MAX_PASTE_ID: usize = 32;

fn id_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_'
}
