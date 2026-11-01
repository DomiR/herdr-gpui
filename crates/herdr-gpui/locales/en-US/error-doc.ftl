# Markdown documents that could not be read, changed, or written. $path is
# a file path; $error is the system's own description; $id is a section's
# ID; $query is what an agent named a todo by.

error-doc-loading = The document is still loading
error-doc-missing = The document's file is gone; save to write it again
error-doc-not-markdown = Only Markdown files open as documents
error-doc-ambiguous-text = { $count } places match the text to replace; include more of it so one does
error-doc-ambiguous-todo = { $count } todos match “{ $query }”; name one by its number
error-doc-bad-item = A todo must be one line of text, at most 4096 characters
error-doc-bad-range = The edit is outside the text or splits a character
error-doc-bad-version = A version is the number the document reports
# $current is the document's version now; $expected the one named.
error-doc-conflict = The document is at version { $current }, not { $expected }
error-doc-crdt = Could not apply a change to the document: { $error }
error-doc-empty-old = Text to replace must not be empty
error-doc-empty-text = There is no text to add
# $operation is a verb such as "read" or "replace", in English.
error-doc-io = Could not { $operation } { $path }: { $error }
error-doc-file-missing = { $path } does not exist; it may have been deleted or moved
error-doc-no-section = No section has the ID { $id }
error-doc-no-text = The text to replace is not in the document
error-doc-no-todo = No todo matches “{ $query }”
error-doc-not-a-file = { $path } is not a regular file
error-doc-not-utf8 = { $path } is not UTF-8 text
# $len and $limit are sizes in bytes.
error-doc-too-large = The document would be { $len } bytes, over the { $limit }-byte limit
