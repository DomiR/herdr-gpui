//! Personal state is read only before the normal app starts its event loop.

use super::{Links, Record};
use gpui::App;
use std::path::PathBuf;

#[derive(Default)]
pub(crate) struct Startup {
    path: Option<PathBuf>,
    records: Vec<Record>,
}

impl Startup {
    pub(crate) fn load() -> Self {
        let Some(path) = crate::preferences::state_dir() else {
            tracing::warn!("Cannot locate document agent links");
            return Self::default();
        };
        let path = path.join(super::FILE);
        match super::load(&path) {
            Ok(records) => Self {
                path: Some(path),
                records,
            },
            Err(error) => {
                tracing::warn!(%error, "Cannot restore document agent links");
                Self {
                    path: Some(path),
                    records: Vec::new(),
                }
            }
        }
    }

    pub(crate) fn install(self, cx: &mut App) {
        Links::install(self.path, self.records, cx);
    }
}
