# Links to the app (sheprd://…) that open a device, a folder, or a
# notification's pane: what the window's status line says when one could
# not be followed. The command line reports the same in English.

# $reason is one of the link-refused-* lines, or why a folder could not be
# opened.
link-open-failed = Open link: { $reason }
link-invalid = That link is not valid
link-refused-closed = The window closed
link-refused-notification-gone = That notification is no longer known
link-refused-dialog-open = A dialog is open in the window; close it first
# $device is the device ID the link names.
link-refused-no-device = No device { $device }
link-refused-not-connected = Sheprd is not connected to this computer's Herdr
# Focus links match names exactly and refuse when a name is gone or shared.
link-refused-gone-agent = No agent matches this link
link-refused-gone-tab = No tab matches this link
link-refused-gone-workspace = No workspace matches this link
link-refused-ambiguous-agent = Several agents match this link; give it a more exact name
link-refused-ambiguous-tab = Several tabs match this link; give it a more exact name
link-refused-ambiguous-workspace = Several workspaces match this link; give it a more exact name
# A checkout the link names is open, but a dialog keeps the window busy.
link-open-dialog-first = Open link: close the open dialog first
