//! Sheprd's icons for a host app built on Moonframe, under the names its
//! mods use: `ui.icon "app-NAME"` loads `icons/app-NAME.svg` from the host,
//! which is Sheprd's `icons/NAME.svg`. The Luau window's mods then draw the
//! same icons as Sheprd's own window, rather than the nearest of Moonframe's.

use super::Icons;
use gpui::{AssetSource, SharedString};
use std::borrow::Cow;

/// What Moonframe puts before an app's own icon names.
const APP: &str = "icons/app-";

/// Sheprd's assets, and each of its icons again as `icons/app-NAME.svg`.
pub(crate) struct HostIcons;

/// The `app-` name a mod gives Sheprd's icon at `path` (`icons/NAME.svg`).
pub(crate) fn app_name(path: &str) -> Option<String> {
    let name = path.strip_prefix("icons/")?.strip_suffix(".svg")?;
    Some(format!("app-{name}"))
}

impl AssetSource for HostIcons {
    fn load(&self, path: &str) -> gpui::Result<Option<Cow<'static, [u8]>>> {
        match path.strip_prefix(APP) {
            Some(name) => Icons.load(&format!("icons/{name}")),
            None => Icons.load(path),
        }
    }

    fn list(&self, path: &str) -> gpui::Result<Vec<SharedString>> {
        let mut found = Icons.list(path)?;
        let renamed: Vec<SharedString> = Icons
            .list("icons/")?
            .iter()
            .filter_map(|icon| icon.strip_prefix("icons/"))
            .map(|name| SharedString::from(format!("{APP}{name}")))
            .filter(|name| name.starts_with(path))
            .collect();
        found.extend(renamed);
        Ok(found)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests;
