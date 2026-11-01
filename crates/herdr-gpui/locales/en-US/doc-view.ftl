# Document tabs: a Markdown file edited in place, with task lists and folds,
# that agents may change at the same time. Document text is never
# translated.

doc-view-no-workspace = Open a workspace first
doc-view-too-many-tabs = Too many tabs are open
doc-view-invalid-path = This file's path cannot be opened as a document
doc-view-loading = Reading file…
# $error says why, in a sentence of its own.
doc-view-failed = Could not open this document: { $error }
doc-view-missing = This file was deleted or moved on disk. The text stays here; save to write it again.
doc-view-write-failed = Could not save: { $error }
# The document's version number, which agents name their changes against.
doc-view-version = Version { $version }
doc-view-hide-done = Hide Done
doc-view-show-done = Show Done
doc-view-fold-all = Fold All
doc-view-unfold-all = Unfold All
# A folded section's or item's badge: how many lines it hides.
doc-view-folded-lines = { $count ->
    [one] 1 line
   *[other] { $count } lines
}
# Done and total tasks under a heading.
doc-view-count = { $done }/{ $total }
# A recent change's label: $who names the agent's pane that made it.
doc-view-changed-by = { $who }
doc-view-changed-on-disk = Changed on disk
# A diagram or formula block while its picture is drawn.
doc-view-drawing = Drawing…
doc-view-diagram-failed = This diagram could not be drawn; click to edit its source
doc-view-formula-failed = This formula could not be drawn; click to edit its source
# Comments on a document's text, sent to the agent that last changed it.
doc-view-comment-placeholder = Comment, then press Enter
doc-view-comment-send = Send to Agent
doc-view-comment-delete = Delete
doc-view-comments-send = { $count ->
    [one] Send 1 Comment
   *[other] Send { $count } Comments
}
