-- The window's layout. Everything here is yours to move, and each part is a
-- mod in mods/: copy one to ~/.config/sheprd/mods to change it.
-- Only the terminals and pages in the middle are drawn by Sheprd in Rust.

local function region(name: string)
	return ui.view { name = "sheprd", id = name, props = { region = name } }
end

-- The agents section's least height, and the room it always leaves the
-- worktrees above it.
local AGENTS_LEAST = 60
local WORKTREES_LEAST = 120

-- The height the left column's lists share: the window's, less the row over
-- them and the device picker under them.
local function lists_height(state: any): number
	return math.max(state.height - state.header - state.devices, 0)
end

-- The agents section's height: as dragged, else half the lists' room as
-- Sheprd's own sidebar starts, kept in bounds as the window changes.
local function agents_height(state: any): number
	local room = lists_height(state)
	local height = state.agents or room / 2
	return math.max(AGENTS_LEAST, math.min(height, room - WORKTREES_LEAST))
end

-- An edge's handle, as Sheprd's own edges draw it (src/panel_resize/grip.rs)
-- after Moonframe's splits: nothing at rest, then a pill on the edge that
-- grows as the pointer hovers, presses and drags it, easing between them
-- (`.sheprd-grip` in style.luau). `look` is nil, "near", "held" or "dragged";
-- `axis` is the way the edge runs.
local function handle(look: string?, axis: string)
	return ui.row {
		class = if axis == "y" then "sheprd-grip sheprd-grip-y" else "sheprd-grip sheprd-grip-x",
		state = look or "idle",
		shrink = false,
	}
end

-- What a hover, press or drag does to an edge's handle.
local function edge_look(look: string?, phase: string): string?
	if phase == "hover" then
		return if look == "held" or look == "dragged" then look else "near"
	elseif phase == "down" then
		return "held"
	elseif phase == "move" then
		return if look then "dragged" else nil
	end
	-- Released or left: the next hover shows it again.
	return nil
end

return mod {
	-- Sheprd's own window opens at 1200 by 780 too.
	window = { size = { width = 1200, height = 780 }, titlebar = "hidden", traffic_lights = { x = 9, y = 9 } },

	init = function(previous)
		-- Sizes come from Sheprd as soon as it starts; the agents section's
		-- height, as dragged, is kept across launches.
		return {
			activity_bar = 48,
			agents = previous and previous.agents,
			devices = 0,
			header = 34,
			height = 780,
			left = 300,
			status_bar = 24,
			titlebar = 0,
		}
	end,

	update = function(state, msg)
		if type(msg) == "table" and msg.left then
			-- Zero is meant: the activity bar turned off, or Zen mode hiding
			-- the left column, and then neither is drawn.
			state.activity_bar = msg.activity_bar
			state.devices = msg.devices or 0
			state.header = msg.header
			state.height = msg.height or state.height
			state.left = msg.left
			state.status_bar = msg.status_bar
			state.titlebar = msg.titlebar or 0
		elseif type(msg) == "table" and msg.sidebar_width then
			-- The column's edge: its handle here, the width in Sheprd.
			-- Returning a table would replace the state, so the drag goes on
			-- as a send beside it.
			state.left_edge = edge_look(state.left_edge, msg.phase)
			if msg.phase == "hover" or msg.phase == "leave" then
				return
			end
			return state, { { to = "provider:sheprd", msg = msg } }
		elseif type(msg) == "table" and msg.agents_edge then
			-- The line over the agents shares the height with the worktrees,
			-- as Sheprd's does; a double click shares it evenly again.
			state.agents_edge = edge_look(state.agents_edge, msg.phase)
			if msg.phase == "down" then
				if msg.clicks == 2 then
					state.agents = nil
					state.dragging = nil
				else
					state.dragging = agents_height(state)
				end
			elseif msg.phase == "move" and state.dragging then
				state.agents = state.dragging - msg.dy
				state.agents = agents_height(state)
			elseif msg.phase == "up" then
				state.dragging = nil
			end
		end
	end,

	view = function(state, app)
		local theme = app.sheprd_theme or {}
		local sheprd = app.sheprd or {}
		-- The worktree and agent lists, unless a page or the rail takes
		-- their place.
		local lists = not sheprd.section and sheprd.sidebar_visible ~= false
		return ui.column {
			grow = true,
			gap = "none",
			style = { background = theme.background },
			ui.row {
				grow = true,
				gap = "none",
				if state.left > 0 then ui.column {
					width = state.left,
					shrink = false,
					gap = "none",
					ui.row { height = state.header, shrink = false, gap = "none", style = { background = theme.sidebar }, ui.column { grow = true, gap = "none", ui.mount "header" } },
					ui.row {
						grow = true,
						gap = "none",
						if state.activity_bar > 0
							then ui.column { width = state.activity_bar, shrink = false, gap = "none", style = { background = theme.sidebar }, ui.mount "activity" }
							else false,
						-- The left pane: the worktrees, new and menu, then the agents,
						-- each a mod, and the device picker under them (`mods/devices`), as Sheprd's
						-- sidebar has it. A page in the sidebar's place shows neither list.
						ui.column {
							class = "sheprd-left",
							grow = true,
							gap = "none",
							style = { background = theme.sidebar },
							ui.column { grow = true, gap = "none", ui.mount "worktrees" },
							-- Under a rule, new and menu, then the agents. The rule is the edge
							-- that shares the height between the worktrees and the agents, as
							-- Sheprd's does.
							if lists then ui.column {
								class = "sheprd-lower",
								shrink = false,
								gap = "none",
								ui.row { class = "sheprd-rule", height = 1, shrink = false, style = { background = theme.active } },
								ui.mount "actions",
								if sheprd.agents_list ~= false then ui.column {
									class = "sheprd-agents-pane",
									height = agents_height(state),
									shrink = false,
									gap = "none",
									ui.mount "agents",
								} else false,
								if sheprd.agents_list ~= false then ui.row {
									class = "sheprd-agents-edge",
									float = { top = -3, left = 0, right = 0 },
									height = 7,
									align = "center",
									justify = "center",
									cursor = "row-resize",
									tooltip = "Drag to resize, double-click to reset",
									pointer = { agents_edge = true },
									hover = true,
									handle(state.agents_edge, "x"),
								} else false,
							} else false,
							if lists and state.devices > 0
								then ui.row { height = state.devices, shrink = false, gap = "none", ui.mount "devices" }
								else false,
						},
					},
				} else false,
				-- The left column's edge, a hairline beside it as Sheprd's border
				-- is. Its grip resizes the column, as Sheprd's does, and a double
				-- click puts back the default width. The grip overhangs the main
				-- area rather than the column, where the lists' scrollbars are.
				if state.left > 0 then ui.column {
					class = "sheprd-left-edge-line",
					width = 1,
					shrink = false,
					ui.row {
						class = "sheprd-left-edge",
						float = { top = 0, bottom = 0, left = 0, right = -6 },
						align = "center",
						justify = "center",
						cursor = "ew-resize",
						tooltip = "Drag to resize, double-click to reset",
						-- To the shell, which draws the handle and passes the drag
						-- on to Sheprd.
						pointer = { sidebar_width = true },
						hover = true,
						handle(state.left_edge, "y"),
					},
				} else false,
				ui.column {
					grow = true,
					gap = "none",
					ui.hsplit {
						id = "body",
						divider = false,
						ui.pane {
							ui.column {
								grow = true,
								gap = "none",
								-- The tabs and terminals, then over the tab strip's
								-- end, the room it leaves free, the title bar's end
								-- (changes, branch, account, the details toggle).
								region "main",
								-- The turns of the chat on screen, over its left
								-- gutter, where Sheprd laid it out.
								if sheprd.chat_turns
									then ui.row {
										float = { top = sheprd.chat_turns.top, left = sheprd.chat_turns.left },
										width = sheprd.chat_turns.width,
										height = sheprd.chat_turns.height,
										gap = "none",
										ui.mount "turns",
									}
									else false,
								-- Drawn before Sheprd has measured it, so it can.
								ui.row {
									float = { top = 0, right = 0 },
									width = math.max(state.titlebar, 1),
									height = state.header,
									gap = "none",
									region "titlebar",
								},
							},
						},
						-- Opened by the toggle that ends Sheprd's title bar.
						ui.pane { size = 300, min = 200, visible = sheprd.details_visible == true, ui.mount "details" },
					},
					ui.row { height = state.status_bar, shrink = false, gap = "none", style = { background = theme.surface }, ui.column { grow = true, gap = "none", ui.mount "status" } },
				},
			},
			-- A worktree build's strip, across the window's foot.
			ui.mount "build",
		}
	end,
}
