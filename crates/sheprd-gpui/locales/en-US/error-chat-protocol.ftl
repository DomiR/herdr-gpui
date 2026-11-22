# The chat attach protocol's failures, shown when a chat or a phone's
# remote control connection receives a frame it cannot use. Entry and
# permission IDs are kept as the protocol writes them, such as EntryId(3).

# $count is how many activities a chat may run at once.
error-chat-protocol-activity-limit = { $count } activities already run
# $bytes is the most bytes an activity's text may take.
error-chat-protocol-activity-text = an activity's text exceeds { $bytes } bytes
# $count is how many briefs, prompts another agent wrote, a chat keeps.
error-chat-protocol-brief-limit = { $count } briefs are already kept
error-chat-protocol-brief-not-prompt = entry { $id } is not a prompt
error-chat-protocol-duplicate-permission = permission request { $id } is already pending
error-chat-protocol-empty-frame = chat frame is empty
error-chat-protocol-entry-order = pushed entry { $id } does not follow the last entry { $last }
# $bytes is the frame's size in bytes.
error-chat-protocol-frame-limit = chat frame of { $bytes } bytes exceeds the limit
error-chat-protocol-invalid-endpoint = not a chat endpoint id
error-chat-protocol-io = chat I/O failed
error-chat-protocol-json = chat message is not valid JSON for this protocol
error-chat-protocol-no-modes = the session has no modes
error-chat-protocol-not-text = entry { $id } has no text to append to
error-chat-protocol-pull-request = not a GitHub pull request, or an action is listed twice
error-chat-protocol-pull-request-limit = { $count } pull requests are already kept
error-chat-protocol-question-answers-limit = question answers in entry { $id } exceed their count or text limit
error-chat-protocol-restore = a chat's restore record could not be used
error-chat-protocol-settings-limit = chat settings of { $bytes } bytes exceed the limit
error-chat-protocol-unknown-entry = no entry { $id }
error-chat-protocol-unknown-permission = no pending permission request { $id }
