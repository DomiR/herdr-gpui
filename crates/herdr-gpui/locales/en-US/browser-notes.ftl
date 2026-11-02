# Notes on a browser tab's page for the agent that opened it: the panel
# beside the page and the messages around sending them.

browser-notes-title = Notes
# The text field a note is written in.
browser-notes-describe = Describe the change…
# The button that makes a drag draw a region over the page.
browser-notes-region = Region
# A note on the whole page rather than on a part of it.
browser-notes-page = Note on page
browser-notes-empty = Click an element, select text, or Shift-drag a region in the page, then describe the change.
browser-notes-capturing = Taking a screenshot…
browser-notes-add = Add note
browser-notes-save = Save note
# Shown in place of a note's text when the page sent a result without one.
browser-notes-placeholder = Add a note…
# A note already sent to the agent. $note is what it points at on the page.
browser-notes-sent = { $note } · sent
browser-notes-withdraw = Withdraw
browser-notes-copy = Copy
browser-notes-clear-sent = Clear sent

# What a note points at. $width and $height are in pixels.
browser-notes-region-size = Region { $width }×{ $height }
browser-notes-whole-page = Whole page
# A recording of the page with the user's narration. $time is its length, such as 1:12.
browser-notes-recording = Recording { $time }
# Something the page itself sent for the agent. $summary is the page's own
# one-line summary of it.
browser-notes-page-result = Page result
browser-notes-page-result-summary = Page result: { $summary }

browser-notes-no-page = No page to annotate
browser-notes-armed = Click an element or select text to note it
browser-notes-full = Clear sent notes or remove some before adding more
browser-notes-result = The page added a result; send it to the agent
browser-notes-empty-note = Write what should change first
browser-notes-screenshots-failed = Screenshots could not be saved
browser-notes-copied = Notes copied
browser-notes-pending = The previous notes are still pending delivery
browser-notes-delivered = The agent already has those notes
browser-notes-withdrawn = Notes withdrawn

# Recording a page review with Scrawl (macOS).
browser-notes-record = Record
browser-notes-record-starting = Starting…
# The button while recording. $time is how long it has run, such as 0:42.
browser-notes-record-stop = Stop { $time }
browser-notes-record-building = Building…
browser-notes-recording-hint = Talk through what should change; hold Shift and drag to draw on the page. Without Shift the page works as usual. Stop builds the recording into a note.
browser-notes-record-busy = Another page is being recorded
browser-notes-record-started = Recording: talk, and hold Shift to draw on the page
browser-notes-record-added = The recording is in the notes; send it to the agent
# $path is the folder Scrawl built the recording into.
browser-notes-record-orphaned = The page closed; its recording is in { $path }
browser-notes-record-no-room = No room for the recording's note; it is in { $path }
