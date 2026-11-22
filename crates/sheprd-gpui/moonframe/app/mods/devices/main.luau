-- The left pane's foot: the device picker, and the sheet it opens above it,
-- as Sheprd's own footer draws them (src/menu/devices/picker/hosted.rs). The
-- sheet lists every device and the sessions of the one unfolded, and the
-- activity of their agents over the last two hours.
--
-- Sheprd keeps what the picker is doing (`app.sheprd.device_picker`): open,
-- the line the keys are on, the device unfolded. This mod draws it and sends
-- back what was clicked and pressed, so the Sessions shortcut, the keys, and
-- every choice behave as in Sheprd's own window. A line is sent by its place
-- among those the keys reach and its key, so a click on a list that changed
-- under the pointer does nothing.
local LINK = "provider:sheprd"
local WIDTH = 280
-- The tallest the list grows before it scrolls, as Sheprd's does.
local LIST_HEIGHT = 420
local SPARK_HEIGHT = 22

-- What a click on a line, its session chip, or its trash asks of Sheprd.
-- The innermost takes the click, so the chip does not also pick its device;
-- Sheprd reads Cmd (Ctrl elsewhere) from the window, as its own picker does.
local function line_click(line: any, how: string)
	return { device_line = line.ordinal, key = line.key, how = how }
end

local function dot(color: string)
	return ui.row { width = 7, height = 7, shrink = false, hug = true, style = { background = color, radius = 999 } }
end

-- The two lines every row shows: its name, bold while current, and its
-- detail under it.
local function labels(line: any, theme: any, color: string)
	return ui.column {
		grow = true,
		gap = "none",
		ui.text { line.label, truncate = true, weight = if line.checked then "bold" else nil, style = { color = color } },
		ui.text { line.detail, size = "small", wrap = true, style = { color = theme.muted } },
	}
end

-- The session a device is on, which unfolds its other sessions under it.
local function chip(line: any, theme: any)
	local chip = line.chip
	return ui.row {
		class = "sheprd-device-chip",
		align = "center",
		shrink = false,
		max_width = 160,
		tooltip = chip.hint,
		cursor = "pointer",
		msg = line_click(line, "sessions"),
		to = LINK,
		style = { gap = 4, padding = { left = 6, right = 4, y = 2 }, radius = 4 },
		if chip.icon
			then ui.icon { name = "layers", size = "small", style = { color = theme.subtext } }
			else ui.row {
				align = "center",
				style = { gap = 4 },
				ui.text { chip.word, size = "small", style = { color = theme.muted } },
				ui.text { chip.session, size = "small", truncate = true, style = { color = theme.subtext } },
			},
		ui.icon { name = if chip.open then "chevron-up" else "chevron-down", size = "small", style = { color = theme.muted } },
	}
end

-- A line the keys reach: All Devices, a device, a session, or Add Device.
local function row(line: any, picker: any, sheet: any, theme: any)
	local color = if line.enabled then theme.foreground else theme.muted
	local background = nil
	if line.checked then
		background = picker.wash
	elseif sheet.selected == line.ordinal then
		background = theme.active
	end
	local leading = if line.kind == "all" then "monitor" elseif line.kind == "add" then "plus" else nil
	local children: { any } = {}
	if leading then
		table.insert(children, ui.icon { name = leading, style = { color = color } })
	end
	table.insert(children, labels(line, theme, color))
	if line.online ~= nil then
		table.insert(children, dot(if line.online then picker.online else theme.muted))
	end
	if line.chip then
		table.insert(children, chip(line, theme))
	end
	if line.delete ~= nil then
		table.insert(children, ui.row {
			class = "sheprd-device-trash",
			shrink = false,
			cursor = "pointer",
			msg = line_click(line, "delete"),
			to = LINK,
			style = { padding = 4, radius = 4 },
			ui.icon { name = "trash-2", size = "small", style = { color = if line.delete then theme.danger else theme.muted } },
		})
	elseif line.kind == "session" then
		table.insert(children, ui.icon { name = "plus", size = "small", style = { color = color } })
	end
	children.class = `sheprd-device-line sheprd-device-{line.kind}`
	children.align = "center"
	children.shrink = false
	children.cursor = if line.enabled then "pointer" else nil
	children.msg = line_click(line, "choose")
	children.to = LINK
	children.style = { gap = 8, padding = 8, radius = 6, background = background }
	return ui.row(children)
end

-- A note under an unfolded device: why its list is short, or what failed.
local function note(line: any, theme: any)
	return ui.text {
		line.text,
		size = "small",
		wrap = true,
		style = { color = if line.kind == "error" then theme.danger else theme.muted, padding = { x = 8, y = 2 } },
	}
end

-- Under its device, a session or note steps in, with a rule tying it there.
local function indented(child: any, theme: any)
	return ui.row {
		gap = "none",
		shrink = false,
		style = { margin = { left = 12 } },
		ui.row { width = 1, shrink = false, style = { background = theme.active } },
		ui.column { grow = true, gap = "none", style = { padding = { left = 6 } }, child },
	}
end

-- The agents' last two hours: working under blocked, scaled to the busiest
-- step, over a faint baseline.
local function spark(activity: any, theme: any)
	local peak = math.max(activity.peak or 0, 1)
	local bars: { any } = { grow = true, height = SPARK_HEIGHT, align = "end", style = { gap = 1 } }
	for _, step in activity.bars or {} do
		if step then
			table.insert(bars, ui.column {
				grow = true,
				gap = "none",
				ui.row { height = SPARK_HEIGHT * step.blocked / peak, style = { background = theme.status.blocked } },
				ui.row { height = SPARK_HEIGHT * step.working / peak, style = { background = theme.status.working } },
			})
		else
			table.insert(bars, ui.row { grow = true })
		end
	end
	return ui.column {
		gap = "none",
		ui.row(bars),
		ui.row { height = 1, style = { background = theme.active } },
	}
end

local function count(text: string, color: string, theme: any)
	return ui.row { align = "center", style = { gap = 5 }, dot(color), ui.text { text, size = "small", style = { color = theme.muted } } }
end

-- Above the list, the activity card; the whole card opens the overview.
local function card(activity: any, theme: any)
	return ui.column {
		class = "sheprd-device-activity",
		shrink = false,
		msg = { devices_overview = true },
		to = LINK,
		cursor = "pointer",
		style = { gap = 6, padding = 10, margin = 4, radius = 6, border = "all", border_color = theme.active, background = theme.background },
		ui.row {
			align = "center",
			style = { gap = 8 },
			ui.row { grow = true, ui.text { activity.heading, size = "small", wrap = true, style = { color = theme.muted } } },
			ui.row {
				align = "center",
				shrink = false,
				style = { gap = 3 },
				ui.text { activity.overview, size = "small", style = { color = theme.subtext } },
				ui.icon { name = "arrow-right", size = "small", style = { color = theme.subtext } },
			},
		},
		spark(activity, theme),
		ui.row {
			align = "center",
			wrap = true,
			style = { gap = 10 },
			count(activity.working, theme.status.working, theme),
			count(activity.blocked, theme.status.blocked, theme),
		},
	}
end

-- The sheet: the card, then the heading and every line, scrolling under it.
local function sheet(picker: any, theme: any)
	local sheet = picker.sheet
	local list: { any } = {
		ui.row {
			align = "center",
			shrink = false,
			style = { gap = 8, padding = 8 },
			ui.row { shrink = false, ui.text { sheet.heading, style = { color = theme.muted } } },
			ui.row { grow = true, ui.text { sheet.hint, size = "small", wrap = true, style = { color = theme.muted } } },
		},
	}
	local reveal = nil
	for _, line in sheet.lines do
		local child
		if line.kind == "note" or line.kind == "error" then
			child = note(line, theme)
		else
			child = row(line, picker, sheet, theme)
		end
		if line.kind == "session" or line.kind == "note" or line.kind == "error" then
			child = indented(child, theme)
		end
		table.insert(list, child)
		if line.ordinal ~= nil and line.ordinal == sheet.selected then
			reveal = #list
		end
	end
	if sheet.adding then
		table.insert(list, ui.text { sheet.adding.heading, style = { color = theme.muted, padding = 8 } })
		for _, job in sheet.adding.jobs do
			table.insert(list, ui.row {
				align = "center",
				shrink = false,
				style = { gap = 8, padding = 8 },
				ui.column {
					grow = true,
					gap = "none",
					ui.text { job.name, truncate = true },
					ui.text { job.detail, size = "small", truncate = true, style = { color = theme.muted } },
				},
				dot(theme.accent),
			})
		end
	end
	list.class = "sheprd-device-list"
	list.scroll = true
	list.max_height = LIST_HEIGHT
	list.scroll_to = reveal
	list.style = { gap = 4 }
	return ui.column {
		class = "sheprd-device-sheet",
		gap = "none",
		if sheet.activity then card(sheet.activity, theme) else false,
		ui.column(list),
	}
end

-- The footer's picker: the device's initials (or the devices icon), what is
-- listed, how many hosts, whether one is connected, and a chevron.
local function picker_button(picker: any, theme: any)
	local footer = picker.footer
	local icon
	if footer.initials then
		icon = ui.row {
			width = 22,
			height = 22,
			align = "center",
			justify = "center",
			shrink = false,
			style = { background = theme.active, radius = 999 },
			ui.text { footer.initials, size = "small", weight = "bold" },
		}
	else
		icon = ui.icon { name = "monitor", style = { color = theme.foreground } }
	end
	if footer.adding then
		-- Cloud machines still being added, on the icon's corner.
		icon = ui.row {
			shrink = false,
			icon,
			ui.row {
				float = { top = -6, right = -8 },
				height = 13,
				align = "center",
				justify = "center",
				style = { background = theme.accent, radius = 999, padding = { x = 3 } },
				ui.text { tostring(footer.adding), size = "small", style = { color = theme.background } },
			},
		}
	end
	return ui.row {
		class = "sheprd-device-picker",
		-- Its own width: the box it floats its sheet from does not grow.
		width = footer.width,
		align = "center",
		cursor = "pointer",
		tooltip = footer.aria,
		msg = { device_picker = true },
		to = LINK,
		style = { gap = 6, padding = 6, radius = 6 },
		icon,
		ui.row { grow = true, ui.text { footer.label, truncate = true } },
		if footer.hosts then ui.row { shrink = false, ui.text { footer.hosts, size = "small", style = { color = theme.muted } } } else false,
		dot(if footer.connected then picker.online else theme.muted),
		ui.icon { name = "chevron-up", size = "small", style = { color = theme.muted } },
	}
end

-- The keys the open sheet takes, as Sheprd's picker reads them.
local KEYS = { "up", "down", "left", "right", "enter", "backspace", "delete", "escape" }

local function keys()
	local keys = {}
	for _, key in KEYS do
		keys[key] = { device_key = key }
	end
	return keys
end

return mod {
	init = function(previous)
		return previous or {}
	end,

	update = function() end,

	view = function(_, app)
		local sheprd = app.sheprd or {}
		local theme = app.sheprd_theme or {}
		local picker = sheprd.device_picker
		if not picker or not picker.footer.shown then
			return ui.row { height = 0 }
		end
		local footer = picker.footer
		return ui.row {
			class = "sheprd-device-footer",
			grow = true,
			align = "center",
			style = { gap = 6, padding = { x = 8 }, background = theme.sidebar, border = "top", border_color = theme.active },
			ui.popover {
				open = picker.open == true and picker.sheet ~= nil,
				dismiss = { device_picker_dismiss = true },
				to = LINK,
				side = "top",
				align = "start",
				gap = 12,
				width = WIDTH,
				keys = keys(),
				content = if picker.sheet then sheet(picker, theme) else ui.row { height = 0 },
				picker_button(picker, theme),
			},
			if footer.settings
				then ui.row {
					class = "sheprd-device-settings",
					width = 28,
					height = 28,
					align = "center",
					justify = "center",
					shrink = false,
					cursor = "pointer",
					tooltip = footer.settings,
					msg = { command = "settings" },
					to = LINK,
					style = { radius = 6 },
					ui.icon { name = "settings", style = { color = theme.foreground } },
				}
				else false,
		}
	end,
}
