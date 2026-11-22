//! The app's own tabs: pages, code, reviews, the orchestrator, and the
//! Devices overview, which Herdr does not know about.
use super::cloud::ExecInfo;
use serde::{Deserialize, Serialize};

/// How long a page may take to hand back its text or picture.
pub const PAGE_WAIT_SECONDS: u64 = 15;
/// How long the window may take to switch to the caller's workspace before
/// opening a tab there.
pub const ARRIVE_WAIT_SECONDS: u64 = 5;
/// The most a script `browser eval` runs in a page may take, in bytes.
pub const MAX_EVAL_SCRIPT_BYTES: usize = 128 * 1024;
/// The most text `machine type` posts at once.
pub const MAX_MACHINE_TYPE_CHARS: usize = 4096;
/// The pause Sheprd Agent takes after each character it types, so apps see
/// them in order.
pub const MACHINE_TYPE_GAP_MS: u64 = 8;
/// How long the longest `machine type` takes in the guest.
pub const MACHINE_TYPE_SECONDS: u64 =
    (MAX_MACHINE_TYPE_CHARS as u64 * MACHINE_TYPE_GAP_MS).div_ceil(1000);
/// How long Sheprd Agent inside a virtual Mac may take to take a picture or
/// post input. Input waits for any typing already under way, as the agent
/// posts one request's input at a time.
pub const MACHINE_AGENT_WAIT_SECONDS: u64 = 30 + MACHINE_TYPE_SECONDS;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum TabOp {
    /// Every app tab, or only browser pages.
    List {
        #[serde(default)]
        pages: bool,
    },
    Focus {
        id: u64,
    },
    Close {
        id: u64,
    },
    /// Load a web address in an existing page tab.
    Navigate {
        id: u64,
        url: String,
    },
    Back {
        id: u64,
    },
    Forward {
        id: u64,
    },
    /// The page's visible text, as the user would select it.
    Text {
        id: u64,
        /// Show the tab first, switching the window to its workspace.
        /// Without it the page is read where it is, out of sight if need
        /// be, and the user's view and keyboard stay where they are.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        focus: bool,
    },
    /// Run `script` in the page and hand back its value as JSON, waiting
    /// up to `wait_seconds` for a promise it returns.
    Eval {
        id: u64,
        script: String,
        wait_seconds: u64,
    },
    /// Forget what WebKit keeps for the page's site: its cache, cookies,
    /// storage, and service workers, on every port of its host.
    ClearData {
        id: u64,
    },
    /// What the page wrote to its console since it loaded, emptied after
    /// reading when `clear` is set.
    Logs {
        id: u64,
        #[serde(default)]
        clear: bool,
    },
    /// A PNG of the page, written to `path`.
    Screenshot {
        id: u64,
        path: String,
        /// Show the tab first, switching the window to its workspace.
        /// Without it only a page already on screen is pictured.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        focus: bool,
    },
    /// How large a page tab draws its page.
    Zoom {
        id: u64,
        change: ZoomChange,
    },
    /// A read-only code tab on a local file, at a 1-based line.
    Code {
        path: String,
        #[serde(default)]
        line: Option<u32>,
    },
    /// The file in the user's terminal editor, in a new pane.
    Edit {
        path: String,
        #[serde(default)]
        line: Option<u32>,
    },
    Devices,
    /// The virtual Mac called `name`, in a tab of the caller's workspace.
    Machine {
        name: String,
    },
    /// Ask in Settings to delete the virtual Mac called `name`. The user
    /// confirms there.
    MachineDelete {
        name: String,
    },
    /// What Sheprd Agent inside the virtual Mac called `name` reports: its
    /// screen, and the permissions macOS gave it.
    MachineAgent {
        name: String,
    },
    /// Click at a point of the screen of the virtual Mac called `name`, in
    /// its screenshot's pixels.
    MachineClick {
        name: String,
        x: u32,
        y: u32,
        #[serde(default)]
        button: MachineButton,
        /// 2 for a double click.
        count: u8,
    },
    /// Run `command` (a program and its arguments) as the logged-in user of
    /// the virtual Mac called `name`, in `cwd` when given, for at most
    /// `wait_seconds`.
    MachineExec {
        name: String,
        command: Vec<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cwd: Option<String>,
        wait_seconds: u64,
    },
    /// Ask in Settings to delete the image called `name`. The user confirms
    /// there.
    MachineImageDelete {
        name: String,
    },
    MachineImageList,
    /// Keep the shut down virtual Mac called `machine` as image `image`.
    MachineImageSave {
        machine: String,
        image: String,
    },
    /// Press `key` in the virtual Mac called `name`, with `modifiers` held.
    MachineKey {
        name: String,
        key: String,
        #[serde(default)]
        modifiers: Vec<MachineModifier>,
    },
    MachineList,
    /// Make the virtual Mac called `name` from image `image`. Without an
    /// image, open Settings > Virtual Macs, where the user confirms making
    /// one from Apple's macOS (named `name`, if given).
    MachineNew {
        #[serde(default)]
        name: Option<String>,
        #[serde(default)]
        image: Option<String>,
    },
    /// Show the virtual Mac called `name` with its setup guide beside it:
    /// the steps that make it ready for agents, which Sheprd checks.
    MachineSetup {
        name: String,
    },
    /// What the virtual Mac called `name` boots with next. Fields left out
    /// keep their value; `display` is `fit` or a size such as `1920x1200`.
    MachineSet {
        name: String,
        #[serde(default)]
        cpus: Option<u16>,
        #[serde(default)]
        memory_gib: Option<u16>,
        #[serde(default)]
        display: Option<String>,
    },
    /// A PNG of the screen of the virtual Mac called `name`, written to
    /// `path`.
    MachineScreenshot {
        name: String,
        path: String,
    },
    /// Save the virtual Mac called `name` and stop it. It resumes when next
    /// shown.
    MachineSuspend {
        name: String,
    },
    /// Type `text` in the virtual Mac called `name`, as from a keyboard.
    MachineType {
        name: String,
        text: String,
    },
    /// The orchestrator of the caller's workspace, or of the checkout at
    /// `repo`.
    Orchestrator {
        #[serde(default)]
        repo: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        action: Option<OrchestratorRun>,
    },
    /// The review of the caller's workspace's uncommitted changes.
    Review,
    Px0,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum OrchestratorRun {
    Adopt { run: String },
    Restart { run: String },
}

impl TabOp {
    pub fn wait_seconds(&self) -> u64 {
        match self {
            Self::Text { .. }
            | Self::Screenshot { .. }
            | Self::Logs { .. }
            | Self::ClearData { .. } => PAGE_WAIT_SECONDS,
            Self::Eval { wait_seconds, .. } => (*wait_seconds).min(super::MAX_WAIT_SECONDS),
            Self::MachineExec { wait_seconds, .. } => {
                wait_seconds.saturating_add(MACHINE_AGENT_WAIT_SECONDS)
            }
            Self::MachineAgent { .. }
            | Self::MachineClick { .. }
            | Self::MachineKey { .. }
            | Self::MachineScreenshot { .. } => MACHINE_AGENT_WAIT_SECONDS,
            Self::MachineType { .. } => MACHINE_AGENT_WAIT_SECONDS + MACHINE_TYPE_SECONDS,
            Self::Code { .. }
            | Self::Edit { .. }
            | Self::Devices
            | Self::Machine { .. }
            | Self::MachineDelete { .. }
            | Self::MachineImageDelete { .. }
            | Self::MachineImageList
            | Self::MachineImageSave { .. }
            | Self::MachineList
            | Self::MachineNew { .. }
            | Self::MachineSet { .. }
            | Self::MachineSetup { .. }
            | Self::Orchestrator { .. }
            | Self::Review
            | Self::Px0 => ARRIVE_WAIT_SECONDS,
            Self::List { .. }
            | Self::Focus { .. }
            | Self::Close { .. }
            | Self::Navigate { .. }
            | Self::Back { .. }
            | Self::Forward { .. }
            | Self::MachineSuspend { .. }
            | Self::Zoom { .. } => 0,
        }
    }
}

/// Which mouse button a click in a virtual Mac presses.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MachineButton {
    #[default]
    Left,
    Right,
}

/// A key held while `machine key` presses another.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MachineModifier {
    // keep-sorted start
    Cmd,
    Control,
    Fn,
    Option,
    Shift,
    // keep-sorted end
}

impl MachineModifier {
    /// The modifier as a key combination spells it: `cmd` in `cmd+q`.
    pub fn parse(name: &str) -> Option<Self> {
        Some(match name {
            "cmd" | "command" => Self::Cmd,
            "control" | "ctrl" => Self::Control,
            "fn" => Self::Fn,
            "option" | "alt" | "opt" => Self::Option,
            "shift" => Self::Shift,
            _ => return None,
        })
    }
}

/// A step in or out, back to 100%, or a zoom of its own, in percent.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "change", rename_all = "snake_case", deny_unknown_fields)]
pub enum ZoomChange {
    In,
    Out,
    Reset,
    Percent { percent: u16 },
}

impl TabKind {
    /// The kind as `--json` spells it, for the text listing.
    pub fn name(self) -> &'static str {
        match self {
            Self::Blank => "blank",
            Self::Web => "web",
            Self::File => "file",
            Self::Code => "code",
            Self::Review => "review",
            Self::Orchestrator => "orchestrator",
            Self::Devices => "devices",
            Self::CoreTerminal => "core_terminal",
            Self::Machine => "machine",
            Self::Editor => "editor",
        }
    }

    /// Whether a page shows it: what `browser list` keeps.
    pub fn is_page(self) -> bool {
        matches!(self, Self::Web | Self::File)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TabKind {
    Blank,
    Web,
    File,
    Code,
    Review,
    Orchestrator,
    Devices,
    /// A herdr-core (Herdr 2) terminal the app draws.
    CoreTerminal,
    /// The workspace's VS Code tab.
    Editor,
    /// A virtual Mac.
    Machine,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TabInfo {
    pub id: u64,
    pub kind: TabKind,
    pub title: String,
    /// The address, file, or checkout the tab shows.
    pub location: Option<String>,
    pub workspace: String,
    /// The host the workspace belongs to: a local socket or an SSH host.
    pub host: String,
    /// The pane that opened the tab, when an agent did.
    pub opened_by: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Reply {
    Tabs {
        tabs: Vec<TabInfo>,
    },
    Opened {
        id: u64,
        workspace: String,
    },
    Text {
        text: String,
    },
    /// What a script run in a page returned, or `None` for `undefined`.
    Eval {
        #[serde(
            default,
            deserialize_with = "present",
            skip_serializing_if = "Option::is_none"
        )]
        value: Option<serde_json::Value>,
    },
    /// A page's console, oldest first, and how many older entries were
    /// dropped to stay within bounds.
    Logs {
        entries: Vec<ConsoleEntry>,
        dropped: u64,
    },
    /// The zoom a page tab draws at now, in percent.
    Zoom {
        id: u64,
        percent: u16,
    },
    Machines {
        machines: Vec<MachineInfo>,
    },
    MachineImages {
        images: Vec<MachineImageInfo>,
    },
    MachineAgent(MachineAgentInfo),
    /// How a command in a virtual Mac ended; `device` is the machine's name.
    MachineExec(Box<ExecInfo>),
    /// A screenshot written to `path`, `width` by `height` pixels.
    MachineScreenshot {
        path: String,
        width: u32,
        height: u32,
    },
}

/// A field that is there, `null` included: only a missing field is `None`.
fn present<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<serde_json::Value>, D::Error> {
    serde_json::Value::deserialize(deserializer).map(Some)
}

/// One line a page wrote to its console, or an error nobody caught.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConsoleEntry {
    pub level: ConsoleLevel,
    pub text: String,
    /// When, in milliseconds since the Unix epoch, by the page's clock.
    pub at: u64,
}

/// The console method that wrote an entry. Uncaught errors and unhandled
/// promise rejections are `Error`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConsoleLevel {
    // keep-sorted start
    Debug,
    Error,
    Info,
    Log,
    Warn,
    // keep-sorted end
}

impl ConsoleLevel {
    /// The level as `--json` spells it, for the text listing.
    pub fn name(self) -> &'static str {
        match self {
            Self::Debug => "debug",
            Self::Error => "error",
            Self::Info => "info",
            Self::Log => "log",
            Self::Warn => "warn",
        }
    }
}

/// What Sheprd Agent inside a virtual Mac reports.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MachineAgentInfo {
    pub version: u32,
    /// The screen in pixels, the unit `machine click` takes.
    pub width: u32,
    pub height: u32,
    /// Pixels per point.
    pub scale: u8,
    /// Whether macOS lets it post input, and take screenshots.
    pub accessibility: bool,
    pub screen_recording: bool,
}

/// What a virtual Mac is doing.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MachineState {
    Stopped,
    /// Stopped with its state saved: it resumes where it was.
    Suspended,
    Starting,
    Running,
    Saving,
    Failed,
}

impl MachineState {
    /// The state as `--json` spells it, for the text listing.
    pub fn name(self) -> &'static str {
        match self {
            Self::Stopped => "stopped",
            Self::Suspended => "suspended",
            Self::Starting => "starting",
            Self::Running => "running",
            Self::Saving => "saving",
            Self::Failed => "failed",
        }
    }
}

/// A virtual Mac installed on this computer.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MachineInfo {
    pub name: String,
    pub state: MachineState,
    /// What it boots with next.
    pub cpus: u16,
    pub memory_gib: u16,
    /// `fit` or a size such as `1920x1200`.
    pub display: String,
    /// What it really takes on disk.
    pub disk_bytes: u64,
}

/// An image new virtual Macs start from.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MachineImageInfo {
    pub name: String,
    /// What its files add up to, shared with the machines made from it.
    pub disk_bytes: u64,
}

#[cfg(test)]
mod tests;
