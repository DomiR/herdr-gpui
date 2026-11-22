-- The row over the left column: room for the traffic lights, then Back and
-- Forward as one control while they fit (`navigation_in_header`); in a
-- narrower column, the rail's, Sheprd's title bar beside it shows them.
local icon_button = use "icon-button"

local function run(command: string)
	return { command = command }
end

return mod {
	init = function(previous)
		return previous or {}
	end,

	update = function() end,

	view = function(_, app)
		local theme = app.sheprd_theme or {}
		local sheprd = app.sheprd or {}
		return ui.column {
			class = "sheprd-header",
			grow = true,
			justify = "center",
			style = { background = theme.sidebar },
			ui.row {
				align = "center",
				gap = "none",
				-- The traffic lights sit here.
				ui.row { width = 90, shrink = false },
				if sheprd.navigation_in_header ~= false then ui.row {
					class = "sheprd-nav",
					align = "center",
					gap = "none",
					shrink = false,
					style = { border = true, border_color = theme.active, radius = 6 },
					icon_button { icon = "chevron-left", size = "small", tooltip = "Back", msg = run "back", to = "provider:sheprd" },
					ui.row { width = 1, height = 14, shrink = false, style = { background = theme.active } },
					icon_button { icon = "chevron-right", size = "small", tooltip = "Forward", msg = run "forward", to = "provider:sheprd" },
				} else false,
			},
		}
	end,
}
