-- Under the worktrees: a new worktree, and the command palette. The shell
-- draws the rule over it, the edge the agents below it are resized by, as
-- in Sheprd's own sidebar.
local LINK = "provider:sheprd"

local function link(text: string, command: string, theme: any)
	return ui.row {
		class = "sheprd-link",
		msg = { command = command },
		to = LINK,
		ui.text { text, style = { color = theme.muted } },
	}
end

return mod {
	init = function(previous)
		return previous or {}
	end,

	update = function() end,

	view = function(_, app)
		local sheprd = app.sheprd or {}
		local theme = app.sheprd_theme or {}
		local layout = sheprd.layout or {}
		if sheprd.section or sheprd.sidebar_visible == false then
			return ui.row { height = 0 }
		end
		return ui.row {
			class = "sheprd-left-actions",
			align = "center",
			shrink = false,
			style = {
				background = theme.sidebar,
				font = "mono",
				size = theme.sidebar_size,
				padding = { x = (layout.padding or 8) + (layout.inset or 0), y = 4 },
			},
			link("new", "new_workspace", theme),
			ui.spacer(),
			link("menu", "command_palette", theme),
		}
	end,
}
