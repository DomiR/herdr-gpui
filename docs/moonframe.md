# Sheprd on Moonframe

Sheprd opens in a window [Moonframe](https://moonframe.dev) lays out: the
`moonframe` feature of `sheprd-gpui`, on by default, so releases ship it. A
plain launch opens this window; `sheprd` commands and `--socket` or `--session`
launches stay the app's own, as without the feature. Releases ship it as a
beta that is on by default: anyone can go back to the classic window as below.
CI builds and tests it with every leg and loads its Luau with `--check`, both
the copy compiled in and the sources.

## The classic window

Turn off **Settings > General > Moonframe window (beta)**
(`moonframe_window = false` under `[features]`) and Sheprd opens its classic window from the next launch.
`sheprd --classic` opens the classic window once, whatever the setting says.
On macOS, hold ⌥ Option as Sheprd starts (from Finder, the Dock, `open`, or
`just run`) to open the other window from the one the setting keeps, without
changing it: the classic window while the setting is on, Moonframe's while it
is off.
The setting is off by default on Windows: Moonframe finds its folders through
`HOME` and the XDG variables, which Windows does not set for the apps it
starts, so it would remember no window placement and write its files to a new
temporary folder at each launch.

The two windows keep their state apart. The classic window's settings, log,
and saved tabs stay under `herdr` as before; Moonframe keeps its own under
the name `sheprd`: `~/.config/sheprd` (your copies of the layout's files),
`~/.cache/sheprd` (the layout written out to run), and `~/.local/state/sheprd`
(where the window was, and each mod's state). Both read the same settings
file and talk to the same daemon.

## Builds

| Build | Layout | The sparkles agent edits |
| --- | --- | --- |
| Default (`just run`, releases) | Compiled in, written to `~/.cache/sheprd/app/<hash>` to run | Your copies in `~/.config/sheprd` |
| `moonframe-studio` (`just run-moonframe`) | Read from `crates/sheprd-gpui/moonframe/app`, reloaded on save | The checkout's files |

Every build has Moonframe's developer tools: Developer Mode, Inspect Element,
and the sparkles agent.

`just run-moonframe` uses your settings and daemon, so quit Sheprd first.

Everything around the terminals is Luau in `crates/sheprd-gpui/moonframe/app`:

| File | Draws |
| --- | --- |
| `main.luau` | The layout: where each part goes, the rule under the worktrees, over New and Menu and the agents, that shares the left pane's height between them (kept across launches; a double click shares it evenly again), and the device picker under them |
| `mods/header/main.luau` | Back and Forward, while they fit the column (`navigation_in_header`); else Sheprd's title bar beside it shows them |
| `mods/activity/main.luau` | The activity bar, in Sheprd's own icons, with the sparkles above Settings |
| `mods/worktrees/main.luau` | The left pane's head and worktree rows, linked worktrees under their checkout, drawn in the sidebar layout Sheprd's config names (`layout = "orca"`, `compact`, `minimal`, ...), and the sidebar toggle in its head, or over the rail of numbered tiles while the sidebar is folded. Dragging the column's edge out of the rail opens the sidebar too |
| `mods/actions/main.luau` | New and Menu under the worktrees: a new worktree, and the command palette |
| `mods/agents/main.luau` | The agents under the worktrees, in Sheprd's sort and the same layout |
| `mods/devices/main.luau` | The device picker at the left pane's foot, and the sheet it opens: the agents' activity, every device and the unfolded one's sessions, and Add Device |
| `mods/details/main.luau` | The right pane; the toggle at the end of the title bar opens it |
| `mods/turns/main.luau` | A chat's turns in its left gutter, as Moonframe's revision timeline draws its chat rail: a tight stack of ticks in the gutter's middle, one per prompt, the prompt and the start of its reply on the card under the pointer, the turn being read marked, and a click back to a prompt. The shell floats it where Sheprd laid the gutter out |
| `mods/status/main.luau` | The status bar |
| `components/sheprd-*.luau` | What the worktrees and agents share: a row highlighted as the layout marks the selected one, the status dot, and the section names |
| `style.luau` | The hover looks, for every mod |

The terminals, tabs, and pages stay one Rust view, placed with
`ui.view { name = "sheprd", id = "main", props = { region = "main" } }`.
The device picker at the left pane's foot is the `devices` mod, shown as
`[sidebar] device_footer` says; Sheprd keeps what it is doing and the dialogs
it opens (Add Device, a session's editor). Sheprd's own picker is still a
region, `region = "devices"`, for a layout that would rather place it.
The title bar's end is Sheprd's own (`region = "titlebar"`): the checkout's changes and
branch, the account, and the details toggle. The shell lays it over the end of
the tab strip, which leaves it that room, at the width Sheprd reports
(`titlebar` among the sizes); each group's tabs, split and ⋯ stay with its
terminals. The terminals keep the keyboard: they take it when the window opens
or comes back to the front, and after a click anywhere in the layout, so only
a field (the search, a note) holds it instead.

## Changing it

In `just run-moonframe`, saving a file reloads the window. To keep a change of
your own in any build, copy the file to the same path under `~/.config/sheprd`
and edit the copy; deleting it brings the original back. The sparkles in the
activity bar, above Settings, open a window where you ask an agent to make a change, and it asks before each edit:
in a release it writes those copies, in `just run-moonframe` the checkout's
files. View > Inspect Element (⌥⌘I, or ⇧⌘X
to go straight to picking) shows what built any part, and its file and line.

Run `just run-moonframe --check` after a change (`sheprd --check` checks the
copy a build carries). It loads what is
drawn at that moment, so a mistake in a row that only appears with an agent
shows in the log when one appears: `unknown key` names it.

## `app.sheprd`

| Field | Holds |
| --- | --- |
| `devices` | Each device the sidebar lists, in its order: `id`, `label`, `selected`, `connected`, `enabled`, `removing`, `collapsed`, `load` (`cpu`, `memory`), `search` (every workspace, folded or not, with `label`, `branch` and `note`, for the sidebar's search), and `workspaces`, its visible rows (fields as below, plus `endpoint`, `group` and `collapsed` on a checkout that folds its worktrees, `note`, `muted`, `teleported`) |
| `multi` | Whether several devices are listed, so each shows a header |
| `device_picker` | The device picker: `footer` (`label`, `initials` beside the activity bar, `hosts`, `connected`, `adding`, `settings` (its tooltip, without the bar), `aria`, `shown`, `width`), `online` and `wash` (the connected dot's and the current line's colors), `open`, and while open `sheet`: `activity` (`heading`, `overview`, `working`, `blocked`, `bars` of `working` and `blocked`, or `false` before a step was seen, and their `peak`), `heading`, `hint`, `selected` (the line the keys are on), `lines` (each `kind`: `all`, `device`, `session`, `add`, `note` or `error`; the lines the keys reach have `ordinal`, `key`, `label`, `detail`, `checked`, `enabled`, and `online`, a device its session `chip` (`word`, `session`, `open`, `hint`), a session `delete` (whether it can be deleted now); a note or error its `text`), and `adding` (cloud machines still being added) |
| `build` | The worktree a build came from: `worktree`, `branch`, `pr` |
| `chat_turns` | The turns of the chat on screen (Herdr's focused pane when several are), or `null` while none shows, its gutter (the room its reading column, `[chat] max_width`, leaves on the left) is under 120 pixels, or it has fewer than two prompts: `pane`; `left`, `top`, `width`, `height`, where the ticks go in the `main` region's coordinates: the gutter's width, and a stack only as tall as the ticks need at `spacing` points apart, in the gutter's middle (the whole gutter once they fill it), since the timeline spreads its ticks over the height it gets; `entries`, each prompt's `id` (its entry), `label` and the start of its reply as `summary`, once there is one; and `selected`, the turn being read |
| `workspaces` | The selected device's, folded or not, in the sidebar's order: `id`, `number`, `label`, `branch`, `repo`, `repo_key`, `linked`, `child` (shown under its checkout), `focused`, `agent_status`, `ahead`, `behind`, `added`, `removed`, `dirty`, `pr`, `pr_color` (the number's color for its state) |
| `agents` | In Sheprd's sort: `name`, `kind` (`claude`, `codex`, ...), `status`, `state` (the status's word, when the agent's integration shows one), `title`, `pane`, `pane_name`, `workspace`, `workspace_label`, `tab`, `focused` |
| `agent_sort` | `grouped` or `priority` |
| `layout` | The configured sidebar layout's `name` and spacing: `padding`, `gap`, `row_padding`, `style_padding`, `child_indent`, `nest` (how far a device's rows step in under its header), `inset`, `spacing`, `radius`, `outline` (rounded highlights), `tree_lines`, `title_case`, `workspace_details`, `child_details`, `pr_counts` |
| `panels`, `panel` | The activity panels (`id`, `label`, `icon`, `count`, `unseen`) and the current one's `id`. `icon` is Sheprd's own, the file its classic bar draws, as an `app-` name (`app-sessions`) that Moonframe loads from the assets Sheprd gives it: each `icons/NAME.svg` of Sheprd's is also `icons/app-NAME.svg` there |
| `connected`, `status` | Whether the daemon answers, and what to say about it |
| `section`, `sidebar_visible`, `status_bar_visible`, `details_visible`, `navigation_in_header` | What the window shows (`navigation_in_header`: whether Back and Forward fit the left column's header, as Sheprd's own window decides); `details_visible` follows the toggle at the end of the title bar, and the shell shows the details pane by it |
| `usage` | The status bar's usage as Sheprd's own bar shows it, or `nil` while hidden or empty: `segments` (each provider's `id`, `name`, `locked`, `stale`, `meter` with `used` as a fraction and its `color`, `labels` with `text` and `color`, and `balance`), `busy`, `error`, and the words `unavailable` and `refresh_label` |

Two parts change on their own, so they have their own providers and a change
runs only the views that read them:

| Provider | Holds |
| --- | --- |
| `app.sheprd_load` | The selected host's load as Sheprd's own status bar shows it, or `nil` while hidden: `cpu` and `memory` as percentages with `cpu_color` and `memory_color`, the labels `cpu_label` and `memory_label` in `label_color`, and while the bar is detailed `history` (the CPU sparkline, oldest first: `value` as a fraction and `color`, or `false` before it is sampled), `meter` (memory: `used`, `color`) and `track`; `unavailable` when the host never answered |
| `app.sheprd_theme` | `accent`, `active`, `added` and `removed` (change counts), `background`, `danger`, `dirty` and `dirty_ink` (the uncommitted-work chip), `foreground`, `muted`, `on_accent` and `on_muted` (text over those), `sidebar`, `sidebar_size` (the named size nearest the sidebar font's size over the UI size), `status` (each agent status's color), `subtext`, `success`, `surface`, `warning` |

Moonframe's own widgets follow Sheprd's theme and UI font without reading these.
`font = "mono"` is Sheprd's sidebar font, though only its family: set
`size = theme.sidebar_size` beside it for the sidebar's size, as the
worktrees and agents do.

## Menus

The menu bar is Sheprd's own, as its window has it (Sheprd, File, Edit, View,
Terminal, Window), with Moonframe's Developer menu before
Window: Developer Mode and, while it is on, Components, Reload Modules, Undo
and Redo, Inspect Element, Developer Tools and Slow Animations. A menu item reaches Sheprd
wherever the focus is, in the Luau layout or a terminal.

## Asking Sheprd for something

Send a message with `to = "provider:sheprd"`:

| Message | Does |
| --- | --- |
| `{ command = "name" }` | Runs any command by its name in the catalog, such as `new_workspace`, `toggle_sidebar`, or `themes` |
| `{ focus_workspace = id }` | Focuses a workspace |
| `{ focus_pane = id }` | Focuses a pane |
| `{ panel = id }` | Shows an activity panel |
| `{ menu = "workspace", workspace = id }` | Opens Sheprd's own popover for a workspace at the pointer, without selecting it: its details, actions (close, delete the worktree, rename, mute, note, teleport, ...) and pull request. Send it as a row's `context`, so a right click opens it |
| `{ menu = "agent", pane = id }` | Opens an agent's pane menu at the pointer |
| `{ menu = "device", endpoint = id }` | Opens a device's menu at the pointer |
| `{ note = id }` | Edits the note on a workspace's checkout |
| `{ toggle_repository = key }` | Folds or unfolds a repository's linked worktrees |
| `{ fold_all = true }` | Folds every repository's worktrees on every device; `false` unfolds them |
| `{ toggle_device = id }`, `{ select_device = id }` | Folds a device's header, or selects the device |
| `{ toggle_agent_sort = true }` | Switches the agents between grouped and by priority |
| `{ move_workspace = id, target = id, after = bool }` | Moves a workspace of the selected device before (or after) another, as dropping its row there does; the worktrees mod sends it when a row with `carry` is dropped on one with `accept` |
| `{ sidebar_width = true }` as a `pointer` | Resizes the sidebar from a drag of its edge (`phase`, `dx`); a double click puts back the default width. The shell draws the edge as a hairline beside the left column, its grip overhanging the main area so it never covers the lists' scrollbars. The shell takes the drag (`hover`), draws the handle as a Moonframe split does (a short pill on hover, a longer one while dragging, as on the rule over New and Menu), and sends the drag on |
| `{ open_build_pr = true }` | Opens the pull request a worktree build came from |
| `{ device_picker = true }`, `{ device_picker_dismiss = true }` | Opens the device picker or closes it, or only closes it (a click outside, Escape) |
| `{ device_line = ordinal, key = key, how = "choose" }` | Picks a picker line: a device (with Cmd, Ctrl elsewhere, listed too or no longer, the sheet left open), a session, or Add Device. `how = "sessions"` unfolds a device's sessions or folds them, `how = "delete"` asks to delete a session. A line whose `key` no longer matches its `ordinal` is left alone |
| `{ chat_turn = pane, revision = id }` | Scrolls the chat in `pane` back to the prompt `id`, as the revision timeline sends it with `select = { chat_turn = pane }`. A prompt the chat no longer has is left alone |
| `{ device_key = key }` | A key the open picker took (`up`, `down`, `left`, `right`, `enter`, `backspace`, `delete`, `escape`), as Sheprd's own picker reads it |
| `{ devices_overview = true }` | Opens the Devices overview, as the picker's activity card does |
| `{ usage = id }`, `{ usage_refresh = true }` | Opens a usage provider's panel above the status bar, or reads usage again |

A row on another device adds `endpoint = id` to its message; without it the
selected device is meant.

## What is not there yet

The window starts as Sheprd's own does (`app::install_services`): it answers
the `sheprd` command line, sends system notifications, follows links, checks
for updates, badges the Dock, and writes Sheprd's log and crash reports. What
looks for a main window finds it through `window::main_window`, whatever the
window's root view.

The window reopens where it was and as large, as Moonframe keeps it
(`window = { remember = false }` in `app/main.luau` turns that off).

Still missing: New Window opens a window of Sheprd's own rather than another
Luau one, until Moonframe can open a second main window. The sidebar has no
device tag sections, no Devices layout or agents nested under their
workspaces, and no menu that opens as the pointer rests on a row.

## Measuring

Compare release builds: a debug window shows a yellow `dev build` strip, and
runs several times slower. GPUI times every frame in either window, so the same
line reads both:

```sh
ZED_MEASUREMENTS=1 RUST_LOG=moonframe::perf=debug target/release/sheprd
ZED_MEASUREMENTS=1 target/release/sheprd --classic
```

`frame duration` is one frame's draw and present. `moonframe::perf` adds what
each publish and message ran. Machine load moves these a lot, so alternate the
two windows and compare medians.

`just bench-moonframe /path/to/herdr` does that for you: it builds the app,
runs each window in turn on a private daemon with 14 linked worktrees, through idle,
terminal output, agents changing state, and focusing each workspace, and
prints the medians of three rounds. It opens windows, so it needs a desktop.
`scripts/bench_frames.py` takes other commands (`NAME=BINARY ARGS...`), round
counts, and `--json`.
