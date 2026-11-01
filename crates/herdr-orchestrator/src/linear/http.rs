//! Linear's GraphQL endpoint: one blocking POST with a personal API key. Only
//! the orchestrator's worker thread calls it. Neither the key nor a response
//! body is ever logged.

use crate::{Error, Result};
use secrecy::{ExposeSecret, SecretString};
use serde_json::{Value, json};
use std::{io::Read, time::Duration};

const URL: &str = "https://api.linear.app/graphql";
const TIMEOUT: Duration = Duration::from_secs(30);
/// Seconds a signed file link stays valid: well past the next sync.
const SIGNED_FOR: &str = "3600";
/// The most of a response read. A full page of issues is far smaller.
pub const LIMIT: usize = 4 * 1024 * 1024;
/// Characters of Linear's own error message kept for display.
const MESSAGE: usize = 240;

/// Sends `query`, one of `linear`'s, with `variables` and returns Linear's
/// response.
pub fn post(key: &SecretString, query: &str, variables: Value) -> Result<Value> {
    let body = json!({ "query": query, "variables": variables }).to_string();
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(TIMEOUT))
        .max_redirects(0)
        .http_status_as_error(false)
        .build()
        .into();
    let mut reply = agent
        .post(URL)
        .header("User-Agent", "Herdr-GPUI")
        .header("Content-Type", "application/json")
        .header("Authorization", authorization(key)?)
        // File links in the answer, such as images in descriptions, come
        // back signed for this long, so they load without the key. Each
        // sync renews them.
        .header("public-file-urls-expire-in", SIGNED_FOR)
        .send(body.as_bytes())
        .map_err(|error| Error::LinearNetwork(Box::new(error)))?;
    let status = reply.status().as_u16();
    let mut bytes = Vec::new();
    reply
        .body_mut()
        .as_reader()
        .take(LIMIT as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(Error::LinearRead)?;
    interpret(status, &bytes)
}

/// A personal API key goes in the header as it is, with no `Bearer`. The
/// header is built from the secret directly, so no plain copy of the key is
/// left in a `String`.
fn authorization(key: &SecretString) -> Result<ureq::http::HeaderValue> {
    let mut header =
        ureq::http::HeaderValue::from_str(key.expose_secret()).map_err(Error::LinearHeader)?;
    header.set_sensitive(true);
    Ok(header)
}

/// What a response with `status` and `body` means: its JSON, or why Linear
/// refused. GraphQL failures can arrive with any status, so the first error's
/// `extensions.code` decides as much as the status does.
pub fn interpret(status: u16, body: &[u8]) -> Result<Value> {
    if body.len() > LIMIT {
        tracing::warn!(
            category = "linear_http",
            status,
            "Linear response exceeded the size limit"
        );
        return Err(Error::LinearSize);
    }
    let parsed = serde_json::from_slice::<Value>(body);
    let error = parsed
        .as_ref()
        .ok()
        .and_then(|value| value.get("errors"))
        .and_then(|errors| errors.get(0));
    if (200..=299).contains(&status) && error.is_none() {
        return parsed.map_err(|source| Error::OutputJson {
            operation: "Reading Linear",
            source,
        });
    }
    let code = error
        .and_then(|error| error.get("extensions"))
        .and_then(|extensions| extensions.get("code"))
        .and_then(Value::as_str);
    tracing::warn!(
        category = "linear_http",
        status,
        code = code.unwrap_or(""),
        "Linear request failed"
    );
    Err(match (status, code) {
        (401, _) | (_, Some("AUTHENTICATION_ERROR")) => Error::LinearKeyRejected,
        (429, _) | (_, Some("RATELIMITED")) => Error::LinearRateLimited,
        _ => Error::LinearStatus {
            status,
            message: error.and_then(message),
        },
    })
}

/// Linear's message as display text. Remote text is untrusted: controls and
/// bidirectional overrides are blanked and the length bounded.
fn message(error: &Value) -> Option<String> {
    let text: String = herdr_github::pull_request::clean(error.get("message")?.as_str()?)
        .trim()
        .chars()
        .take(MESSAGE)
        .collect();
    (!text.is_empty()).then_some(text)
}
