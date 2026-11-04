# Document agent link failures. Variables carry paths, limits, seconds, kinds,
# fields, and selector data, not English error text.

error-doc-link-agent-command = invalid agent command
error-doc-link-agent-command-control = an agent command argument contains a control character
error-doc-link-agent-command-empty = the agent command is empty
# $kind is the configured agent kind.
error-doc-link-agent-command-kind = unknown agent kind { $kind }
# $limit is the maximum number of command arguments.
error-doc-link-agent-command-too-many-arguments = the agent command has more than { $limit } arguments
# $limit is the maximum command size in bytes.
error-doc-link-agent-command-too-many-bytes = the agent command exceeds { $limit } bytes
error-doc-link-agent-gone = the document agent is gone
error-doc-link-busy = the previous document agent operation is still stopping
error-doc-link-cancelled = the document agent link was cancelled
# $seconds is the configured debounce duration.
error-doc-link-debounce = document agent debounce must be between 1 and 300 seconds, got { $seconds }
error-doc-link-decode = saved document agent links are not valid JSON
# $path is the document buffer's path.
error-doc-link-document-unavailable = the document at { $path } could not be loaded
error-doc-link-invalid-agent-name = the document agent name is invalid
error-doc-link-invalid-record = a saved document agent link is invalid

error-doc-link-name-taken = the document agent name is already taken
error-doc-link-not-ready = the document agent is not ready

error-doc-link-output =
    { $step ->
        [create-tab] creating the document agent tab failed: unexpected output
        [pane-query] checking the document agent pane failed: unexpected output
        [prompt] sending the document agent prompt failed: unexpected output
       *[start-agent] starting the document agent failed: unexpected output
    }

error-doc-link-record-control-character = the { $field } field contains a control character
error-doc-link-record-duplicate-path = a document path appears more than once
error-doc-link-record-empty-field = the { $field } field must not be empty
error-doc-link-record-field-too-long = the { $field } field exceeds { $limit } bytes
error-doc-link-record-invalid-agent-name = the agent name is invalid
error-doc-link-record-malformed = a saved link record is malformed
error-doc-link-record-path-too-long = the document path exceeds { $limit } bytes
error-doc-link-record-relative-path = the document path must be absolute

error-doc-link-schema = unsupported document agent link schema version { $version }
# $error is the host script's localized failure.
error-doc-link-script =
    { $step ->
        [create-tab] creating the document agent tab failed: { $error }
        [pane-query] checking the document agent pane failed: { $error }
        [prompt] sending the document agent prompt failed: { $error }
       *[start-agent] starting the document agent failed: { $error }
    }

# $path is the saved link file path.
error-doc-link-storage = could not access saved document agent links at { $path }
# $limit is the maximum number of saved links.
error-doc-link-too-many = too many document agent links, limit is { $limit }
error-doc-link-unavailable = the document's Herdr workspace is not connected
error-doc-link-unsupported = Agent links need a local Herdr 1 session
error-doc-link-unsupported-target = this target has no local Herdr 1 agent API
