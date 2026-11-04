use super::{Args, CliError, LinkTarget, unknown};
use std::ffi::OsString;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LinkCommand {
    pub target: LinkTarget,
    pub pane: Option<String>,
}

pub(super) fn link(args: &mut Args) -> Result<LinkCommand, CliError> {
    let target = match args.verb().as_ref().and_then(|verb| verb.to_str()) {
        Some("agent") => LinkTarget::Agent,
        Some("tab") => LinkTarget::Tab,
        Some("workspace") => LinkTarget::Workspace,
        Some(verb) => return Err(unknown("link", OsString::from(verb))),
        None => return Err(CliError::InvalidValue("workspace|tab|agent")),
    };
    Ok(LinkCommand {
        pane: args.text_option("--pane")?,
        target,
    })
}
