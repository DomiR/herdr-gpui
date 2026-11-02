//! `herdr-gpui://` links: what the Windows shell hands the app when the user
//! picks a jump list entry, clicks a notification, or follows a link from a
//! browser or another app.
//!
//! A link is untrusted input. Any web page can link one, and any local
//! process can send one, so this is the only parser and it accepts exactly:
//!
//! - `herdr-gpui://open?path=PATH`: a local checkout or worktree to show;
//! - `herdr-gpui://device?id=ID`: a device (host) to switch to;
//! - `herdr-gpui://focus?...`: a workspace, tab, or agent pane to focus;
//! - `herdr-gpui://notification?tag=TAG`: the notification that was clicked.
//!
//! Nothing else: no other parts, no unknown or repeated parameters, bounded
//! lengths, and no control characters. A link names something to show; it
//! never carries a command, and the app never runs one because of it.

pub(crate) mod focus;
pub(crate) mod follow;
#[cfg(test)]
mod tests;

use std::path::{Path, PathBuf};
use url::Url;

pub(crate) const SCHEME: &str = "herdr-gpui";
/// Bytes in a whole link. Browsers allow far more; nothing here needs it.
const MAX_LINK: usize = 4096;
const MAX_PATH: usize = 1024;
const MAX_DEVICE: usize = 256;
const MAX_TAG: usize = 512;
const MAX_FOCUS_TEXT: usize = 256;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Focus {
    pub(crate) path: Option<PathBuf>,
    pub(crate) workspace: Option<String>,
    pub(crate) tab_or_agent: Option<Below>,
    pub(crate) id: Option<String>,
    pub(crate) device: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Below {
    Agent(String),
    Tab(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Link {
    Open { path: PathBuf },
    Device { id: String },
    Focus(Focus),
    Notification { tag: String },
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub(crate) enum Error {
    #[error("the link is longer than {MAX_LINK} bytes")]
    TooLong,
    #[error("not a {SCHEME}:// link")]
    Scheme,
    #[error("the link is malformed")]
    Malformed(#[source] url::ParseError),
    #[error("the link has a part {SCHEME}:// links never have")]
    UnexpectedPart,
    #[error("unknown link action {0:?}")]
    UnknownAction(String),
    #[error("unknown link parameter {0:?}")]
    UnknownParameter(String),
    #[error("the link repeats {0}")]
    DuplicateParameter(&'static str),
    #[error("the link needs {0}")]
    MissingParameter(&'static str),
    #[error("the link's path is not an absolute local path")]
    Path,
    #[error("the link's device is not a device ID")]
    Device,
    #[error("the link's notification is not one this app posts")]
    Tag,
    #[error("the focus link needs a path, workspace, or agent")]
    FocusTargetMissing,
    #[error("the focus link cannot name both a tab and an agent")]
    FocusTabAndAgent,
    #[error("the focus link's {0} is empty, too long, or contains a control character")]
    FocusText(&'static str),
    #[error("the focus link's ID is invalid")]
    FocusId,
    #[error("the focus link's device is not a device ID")]
    FocusDevice,
}

/// Whether a command-line argument is a link rather than an option, so a
/// malformed one is reported as a link instead of an unknown option.
pub(crate) fn is_link(argument: &std::ffi::OsStr) -> bool {
    argument.as_encoded_bytes().len() > SCHEME.len()
        && argument.as_encoded_bytes()[..=SCHEME.len()]
            .eq_ignore_ascii_case(format!("{SCHEME}:").as_bytes())
}

impl Link {
    pub(crate) fn parse(text: &str) -> Result<Self, Error> {
        if text.len() > MAX_LINK {
            return Err(Error::TooLong);
        }
        let url = Url::parse(text).map_err(Error::Malformed)?;
        if url.scheme() != SCHEME {
            return Err(Error::Scheme);
        }
        if !url.username().is_empty()
            || url.password().is_some()
            || url.port().is_some()
            || url.fragment().is_some()
            || !matches!(url.path(), "" | "/")
        {
            return Err(Error::UnexpectedPart);
        }
        let action = url.host_str().unwrap_or_default();
        if !matches!(action, "open" | "device" | "focus" | "notification") {
            return Err(Error::UnknownAction(crate::notifications::safe_text(
                action, 64,
            )));
        }
        let mut parameters = Parameters::default();
        for (key, value) in url.query_pairs() {
            parameters.add(&key, value.into_owned())?;
        }
        match action {
            "open" => {
                parameters.only(&["path"])?;
                Ok(Self::Open {
                    path: local_path(&parameters.take("path")?)?,
                })
            }
            "device" => {
                parameters.only(&["id"])?;
                let id = parameters.take("id")?;
                Ok(Self::Device { id: device_id(id)? })
            }
            "focus" => {
                parameters.only(&["agent", "device", "id", "path", "tab", "workspace"])?;
                let path = parameters
                    .take_optional("path")?
                    .map(|path| local_path(&path))
                    .transpose()?;
                let workspace = parameters
                    .take_optional("workspace")?
                    .map(|workspace| focus_text("workspace", workspace))
                    .transpose()?;
                let tab = parameters
                    .take_optional("tab")?
                    .map(|tab| focus_text("tab", tab))
                    .transpose()?;
                let agent = parameters
                    .take_optional("agent")?
                    .map(|agent| focus_text("agent", agent))
                    .transpose()?;
                if tab.is_some() && agent.is_some() {
                    return Err(Error::FocusTabAndAgent);
                }
                let id = parameters
                    .take_optional("id")?
                    .map(validate_focus_id)
                    .transpose()?;
                let device = parameters
                    .take_optional("device")?
                    .map(|device| device_id(device).map_err(|_| Error::FocusDevice))
                    .transpose()?;
                if path.is_none() && workspace.is_none() && agent.is_none() {
                    return Err(Error::FocusTargetMissing);
                }
                Ok(Self::Focus(Focus {
                    path,
                    workspace,
                    tab_or_agent: tab.map(Below::Tab).or_else(|| agent.map(Below::Agent)),
                    id,
                    device,
                }))
            }
            "notification" => {
                parameters.only(&["tag"])?;
                let tag = parameters.take("tag")?;
                // `herdr:` for daemon notices, `herdr-preview:` for the QA one.
                if tag.len() > MAX_TAG
                    || !tag.starts_with("herdr")
                    || tag.chars().any(crate::notifications::unsafe_char)
                {
                    return Err(Error::Tag);
                }
                Ok(Self::Notification { tag })
            }
            other => Err(Error::UnknownAction(crate::notifications::safe_text(
                other, 64,
            ))),
        }
    }

    /// The link as text, which [`Link::parse`] reads back to the same link.
    pub(crate) fn to_url(&self) -> String {
        let (action, key, value) = match self {
            Self::Open { path } => ("open", "path", path.to_string_lossy()),
            Self::Device { id } => ("device", "id", id.as_str().into()),
            Self::Notification { tag } => ("notification", "tag", tag.as_str().into()),
            Self::Focus(focus) => return focus_url(focus),
        };
        let query: String = url::form_urlencoded::Serializer::new(String::new())
            .append_pair(key, &value)
            .finish();
        format!("{SCHEME}://{action}?{query}")
    }
}

fn focus_url(focus: &Focus) -> String {
    let mut query = url::form_urlencoded::Serializer::new(String::new());
    if let Some(path) = &focus.path {
        query.append_pair("path", &path.to_string_lossy());
    }
    if let Some(workspace) = &focus.workspace {
        query.append_pair("workspace", workspace);
    }
    if let Some(below) = &focus.tab_or_agent {
        match below {
            Below::Tab(tab) => query.append_pair("tab", tab),
            Below::Agent(agent) => query.append_pair("agent", agent),
        };
    }
    if let Some(id) = &focus.id {
        query.append_pair("id", id);
    }
    if let Some(device) = &focus.device {
        query.append_pair("device", device);
    }
    format!("{SCHEME}://focus?{}", query.finish())
}

/// The query, each key at most once.
#[derive(Default)]
struct Parameters(Vec<(&'static str, String)>);

const KEYS: &[&str] = &["agent", "device", "id", "path", "tab", "tag", "workspace"];

impl Parameters {
    fn add(&mut self, key: &str, value: String) -> Result<(), Error> {
        let Some(key) = KEYS.iter().copied().find(|known| *known == key) else {
            return Err(Error::UnknownParameter(crate::notifications::safe_text(
                key, 64,
            )));
        };
        if self.0.iter().any(|(seen, _)| *seen == key) {
            return Err(Error::DuplicateParameter(key));
        }
        self.0.push((key, value));
        Ok(())
    }

    /// Refuses a key this action does not take.
    fn only(&self, allowed: &[&str]) -> Result<(), Error> {
        match self.0.iter().find(|(key, _)| !allowed.contains(key)) {
            Some((key, _)) => Err(Error::UnknownParameter((*key).to_owned())),
            None => Ok(()),
        }
    }

    fn take(&mut self, key: &'static str) -> Result<String, Error> {
        self.take_optional(key)?.ok_or(Error::MissingParameter(key))
    }

    fn take_optional(&mut self, key: &'static str) -> Result<Option<String>, Error> {
        let at = self.0.iter().position(|(seen, _)| *seen == key);
        Ok(at.map(|at| self.0.swap_remove(at).1))
    }
}

fn device_id(id: String) -> Result<String, Error> {
    if id.len() > MAX_DEVICE || !id.chars().all(|c| c.is_ascii_graphic()) {
        return Err(Error::Device);
    }
    Ok(id)
}

fn focus_text(kind: &'static str, text: String) -> Result<String, Error> {
    if text.is_empty() || text.len() > MAX_FOCUS_TEXT || text.chars().any(char::is_control) {
        return Err(Error::FocusText(kind));
    }
    Ok(text)
}

fn validate_focus_id(id: String) -> Result<String, Error> {
    focus_text("id", id).map_err(|_| Error::FocusId)
}

/// An absolute path on this machine, refusing anything that would reach
/// another one or a device: on Windows, a drive path only, never a UNC or
/// `\\?\` path, since merely looking at `\\server\share` sends the user's
/// credentials to that server. No `..`, so what is shown is what is opened.
fn local_path(text: &str) -> Result<PathBuf, Error> {
    if text.is_empty() || text.len() > MAX_PATH || text.chars().any(char::is_control) {
        return Err(Error::Path);
    }
    if !platform_path(text) {
        return Err(Error::Path);
    }
    let path = Path::new(text);
    if !path.is_absolute()
        || path
            .components()
            .any(|part| matches!(part, std::path::Component::ParentDir))
    {
        return Err(Error::Path);
    }
    Ok(path.to_owned())
}

pub(crate) fn validated_local_path(text: &str) -> Option<PathBuf> {
    local_path(text).ok()
}

#[cfg(windows)]
fn platform_path(text: &str) -> bool {
    let bytes = text.as_bytes();
    bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && matches!(bytes[2], b'\\' | b'/')
        // A second colon names an alternate data stream; the others are
        // never in a Windows file name.
        && !text[2..].contains([':', '<', '>', '"', '|', '?', '*'])
}

#[cfg(not(windows))]
fn platform_path(text: &str) -> bool {
    text.starts_with('/') && !text.starts_with("//")
}
