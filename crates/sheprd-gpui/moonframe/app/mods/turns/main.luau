-- The turns of the chat on screen, in its left gutter: one tick per prompt,
-- the prompt and the start of its reply on the card under the pointer, and
-- the turn being read marked. A click scrolls the conversation back to that
-- prompt (src/chat/turns.rs).
--
-- Sheprd says where the ticks go (`app.sheprd.chat_turns`), as it lays the
-- chat out: a tight stack in the gutter's middle, only as tall as they need,
-- since the timeline spreads its ticks over the height it gets. The layout
-- floats this mod there.
local LINK = "provider:sheprd"

return mod {
	init = function(previous)
		return previous or {}
	end,

	update = function() end,

	view = function(_, app)
		local turns = (app.sheprd or {}).chat_turns
		if not turns then
			return ui.row { height = 0 }
		end
		return ui.revision_timeline {
			entries = turns.entries,
			selected = turns.selected,
			side = "left",
			dates = false,
			labels = "hover",
			preview = true,
			width = turns.width,
			height = turns.height,
			-- As Moonframe's gallery draws the chat rail.
			tick_width = 6,
			spacing = turns.spacing,
			magnification = 4,
			radius = 3,
			-- The timeline adds the prompt's entry id as `revision`.
			select = { chat_turn = turns.pane },
			to = LINK,
		}
	end,
}
