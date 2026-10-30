//! `mod.toml`: a mod's name, where it appears, the palette commands it adds,
//! and what it needs beyond drawing and catalog commands.
//!
//! ```toml
//! name = "pr-badge"
//! api = 1
//! places = ["status_bar", "sidebar"]
//! commands = [{ id = "refresh", title = "Refresh PR badge" }]
//!
//! [activity]
//! title = "Deploys"
//! icon = "icon.svg"
//!
//! [needs]
//! exec = ["gh"]
//! ```

use crate::{Error, Problem, Result, names::named};
use serde::Deserialize;

/// The mod API this build offers. A manifest names the one it was written
/// for, so a later incompatible API can refuse it clearly.
pub const API: u32 = 1;
const MAX_NAME: usize = 40;
const MAX_TITLE: usize = 60;
const MAX_COMMANDS: usize = 16;
const MAX_NEEDS: usize = 16;
const MAX_NEED: usize = 256;

named! {
    /// Where a mod's view is drawn. Activity bar apps are `[activity]`.
    pub enum Place {
        Sidebar = "sidebar",
        StatusBar = "status_bar",
        Tab = "tab",
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Manifest {
    pub name: String,
    pub places: Vec<Place>,
    pub commands: Vec<CommandDecl>,
    pub activity: Option<Activity>,
    pub needs: Needs,
}

/// A palette command; running it sends the mod the message `command:<id>`.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CommandDecl {
    pub id: String,
    pub title: String,
}

/// An icon in the activity bar that opens the mod as a full app.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Activity {
    pub title: String,
    /// An SVG file in the mod's folder.
    pub icon: String,
}

/// What a mod asks for beyond drawing and catalog commands. Anything listed
/// needs a person's approval, pinned to the mod's hash.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Needs {
    #[serde(default)]
    pub exec: Vec<String>,
    #[serde(default)]
    pub network: Vec<String>,
    #[serde(default)]
    pub read: Vec<String>,
}

impl Needs {
    pub fn is_empty(&self) -> bool {
        self.exec.is_empty() && self.network.is_empty() && self.read.is_empty()
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Raw {
    name: String,
    api: u32,
    #[serde(default)]
    places: Vec<String>,
    #[serde(default)]
    commands: Vec<CommandDecl>,
    activity: Option<Activity>,
    #[serde(default)]
    needs: Needs,
}

impl Manifest {
    pub fn parse(source: &str) -> Result<Self> {
        let raw: Raw = toml::from_str(source)?;
        let invalid = |at: &str, problem| Error::Manifest {
            at: at.to_owned(),
            problem,
        };
        if raw.api != API {
            return Err(invalid("api", Problem::OneOf(&["1"])));
        }
        identifier(&raw.name).map_err(|problem| invalid("name", problem))?;
        let mut places = Vec::with_capacity(raw.places.len());
        for (index, place) in raw.places.iter().enumerate() {
            let place = Place::parse(place).ok_or_else(|| {
                invalid(&format!("places[{index}]"), Problem::OneOf(Place::NAMES))
            })?;
            if !places.contains(&place) {
                places.push(place);
            }
        }
        if raw.commands.len() > MAX_COMMANDS {
            let limit = MAX_COMMANDS;
            return Err(invalid("commands", Problem::Entries { limit }));
        }
        for (index, command) in raw.commands.iter().enumerate() {
            identifier(&command.id).map_err(|p| invalid(&format!("commands[{index}].id"), p))?;
            if raw.commands[..index]
                .iter()
                .any(|seen| seen.id == command.id)
            {
                return Err(invalid(
                    &format!("commands[{index}].id"),
                    Problem::Expected { expected: "unique" },
                ));
            }
            title(&command.title).map_err(|p| invalid(&format!("commands[{index}].title"), p))?;
        }
        if let Some(activity) = &raw.activity {
            title(&activity.title).map_err(|p| invalid("activity.title", p))?;
            icon_file(&activity.icon).map_err(|p| invalid("activity.icon", p))?;
        }
        for (field, list) in [
            ("needs.exec", &raw.needs.exec),
            ("needs.network", &raw.needs.network),
            ("needs.read", &raw.needs.read),
        ] {
            if list.len() > MAX_NEEDS {
                return Err(invalid(field, Problem::Entries { limit: MAX_NEEDS }));
            }
            for (index, need) in list.iter().enumerate() {
                text(need, MAX_NEED).map_err(|p| invalid(&format!("{field}[{index}]"), p))?;
            }
        }
        Ok(Self {
            name: raw.name,
            places,
            commands: raw.commands,
            activity: raw.activity,
            needs: raw.needs,
        })
    }
}

/// Lowercase ASCII letters, digits, and inner hyphens, starting with a
/// letter: safe as a folder name, a palette prefix, and a config key.
pub(crate) fn identifier(name: &str) -> Result<(), Problem> {
    if name.is_empty() {
        return Err(Problem::Empty);
    }
    if name.len() > MAX_NAME {
        return Err(Problem::TooLong { limit: MAX_NAME });
    }
    let valid = name.starts_with(|c: char| c.is_ascii_lowercase())
        && !name.ends_with('-')
        && !name.contains("--")
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
    if !valid {
        return Err(Problem::Expected {
            expected: "lowercase letters, digits, and single hyphens, starting with a letter",
        });
    }
    Ok(())
}

fn title(value: &str) -> Result<(), Problem> {
    text(value, MAX_TITLE)
}

/// Non-empty, bounded text with no control characters.
pub(crate) fn text(value: &str, limit: usize) -> Result<(), Problem> {
    if value.trim().is_empty() {
        return Err(Problem::Empty);
    }
    if value.len() > limit {
        return Err(Problem::TooLong { limit });
    }
    if value.chars().any(char::is_control) {
        return Err(Problem::Control);
    }
    Ok(())
}

/// A plain `.svg` file name in the mod's own folder.
pub(crate) fn icon_file(name: &str) -> Result<(), Problem> {
    let stem = name.strip_suffix(".svg").ok_or(Problem::Expected {
        expected: "an .svg file in the mod's folder",
    })?;
    identifier(stem)
}
