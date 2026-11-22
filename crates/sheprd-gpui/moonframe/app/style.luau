-- Rows answer the pointer as Sheprd's do. How a selected row is marked
-- depends on the sidebar layout, so `sheprd-row` draws it from
-- `app.sheprd.layout`; colors here are the toolkit's, which follow Sheprd's
-- theme: `muted` is its selection, `muted-foreground` its quiet text.
return style {
	rules = {
		[".worktree-tile"] = { radius = 6 },
		-- The worktrees head's fold and search, as small icon buttons are.
		[".sheprd-head-tool"] = { radius = 6 },
		[".worktree-tile:selected"] = { background = "muted" },
		[".worktree-row:hover"] = { background = "secondary" },
		[".agent-row:hover"] = { background = "secondary" },
		[".activity-tile:hover"] = { background = "secondary" },
		-- The sparkles' button (the feedback mod's) as an activity tile.
		[".feedback-button"] = { width = 32, height = 32, padding = 0, radius = 8, size = "title" },
		[".status-item:hover"] = { opacity = 0.75 },
		[".sheprd-link:hover"] = { opacity = 0.75 },
		-- The device footer and its sheet, as Sheprd's own answer the pointer.
		[".sheprd-device-picker:hover"] = { background = "secondary" },
		[".sheprd-device-settings:hover"] = { background = "secondary" },
		[".sheprd-device-line:hover"] = { background = "secondary" },
		[".sheprd-device-chip:hover"] = { background = "secondary" },
		[".sheprd-device-trash:hover"] = { background = "secondary" },
		[".sheprd-device-activity:hover"] = { border_color = "muted-foreground" },
		-- The left pane's edge: a hairline; the shell draws its handle.
		[".sheprd-left-edge-line"] = { background = "border" },
		-- An edge's handle, as Sheprd's own (src/panel_resize/grip.rs): its
		-- accent pill, 3 across, growing and easing in over 180ms.
		[".sheprd-grip"] = { background = "#78a9ff", radius = 999, opacity = 0, transition = "fade 180ms ease-out" },
		[".sheprd-grip-x"] = { width = 0, height = 3 },
		[".sheprd-grip-y"] = { width = 3, height = 0 },
		[".sheprd-grip-x:near"] = { width = 20, opacity = 0.35 },
		[".sheprd-grip-x:held"] = { width = 28, opacity = 0.6 },
		[".sheprd-grip-x:dragged"] = { width = 44, opacity = 0.9 },
		[".sheprd-grip-y:near"] = { height = 20, opacity = 0.35 },
		[".sheprd-grip-y:held"] = { height = 28, opacity = 0.6 },
		[".sheprd-grip-y:dragged"] = { height = 44, opacity = 0.9 },
	},
}
