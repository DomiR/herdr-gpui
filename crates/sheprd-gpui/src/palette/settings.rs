//! The `[palette]` config table: the double-Shift gesture and where the
//! Projects filter finds its folders.

use super::Error;
use crate::Result;

/// Bounds a configured path shares with the project roots.
const MAX_PATH: usize = 8192;
const MAX_ROOTS: usize = 16;
const MAX_LISTED: usize = 256;
const MAX_NAME: usize = 256;

#[derive(Clone, Debug, serde::Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct PaletteConfig {
    pub double_shift: bool,
    /// Folders whose immediate children are each a project.
    pub project_roots: Vec<String>,
    /// Folders that are each a project, wherever they are.
    pub projects: Vec<ProjectConfig>,
}

/// One folder listed as a project by itself, named for its folder unless
/// `name` says otherwise.
#[derive(Clone, Debug, serde::Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ProjectConfig {
    pub path: String,
    #[serde(default)]
    pub name: Option<String>,
}

impl Default for PaletteConfig {
    fn default() -> Self {
        Self {
            double_shift: true,
            project_roots: Vec::new(),
            projects: Vec::new(),
        }
    }
}

fn valid_text(text: &str, max: usize) -> bool {
    !text.trim().is_empty() && text.len() <= max && !text.chars().any(char::is_control)
}

impl PaletteConfig {
    pub(crate) fn validate(&self) -> Result<()> {
        if self.project_roots.len() > MAX_ROOTS
            || !self
                .project_roots
                .iter()
                .all(|root| valid_text(root, MAX_PATH))
        {
            return Err(Error::ProjectRoots.into());
        }
        if self.projects.len() > MAX_LISTED
            || !self.projects.iter().all(|project| {
                valid_text(&project.path, MAX_PATH)
                    && project
                        .name
                        .as_deref()
                        .is_none_or(|name| valid_text(name, MAX_NAME))
            })
        {
            return Err(Error::ListedProjects.into());
        }
        Ok(())
    }

    /// Whether the Projects filter has any folder to look for.
    pub(crate) fn has_projects(&self) -> bool {
        !self.project_roots.is_empty() || !self.projects.is_empty()
    }
}
