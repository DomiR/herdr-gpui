import SheprdRemoteCore

/// What the phone asks a computer to do to its chats, tabs, and
/// workspaces, each one call to the daemon at the address its link uses.
extension Fleet {
    /// Closes the pane a chat runs in, which stops its agent in Sheprd.
    func closeChatPane(_ key: PaneKey) async throws {
        let pane = key.pane
        _ = try await call(key.device) { client, token in
            try await client.termAction(.closePane, pane: pane, bearer: token)
        }
    }

    /// Stars or unstars the agent in `key`'s pane on its computer, which
    /// every device then lists first.
    func setFavorite(_ favorite: Bool, for key: PaneKey) async throws {
        let pane = key.pane
        try await call(key.device) { client, token in
            try await client.setFavorite(favorite, pane: pane, bearer: token)
        }
    }

    /// Asks for a chat's row summary; every phone's list shows it when the
    /// computer has it.
    func summarize(_ key: PaneKey) async throws {
        let pane = key.pane
        try await call(key.device) { client, token in
            try await client.summarize(pane: pane, bearer: token)
        }
    }

    /// A short title for a chat from Claude, run once on its own.
    func suggestTitle(_ key: PaneKey) async throws -> String {
        let pane = key.pane
        return try await call(key.device) { client, token in
            try await client.suggestTitle(pane: pane, bearer: token)
        }
    }

    /// Closes the terminal tab a pane is in, with every pane in it.
    func closeTab(_ key: PaneKey) async throws {
        let pane = key.pane
        _ = try await call(key.device) { client, token in
            try await client.termAction(.closeTab, pane: pane, bearer: token)
        }
    }

    /// Names a chat or the terminal tab a pane is in; the next `home` from
    /// the computer carries the name to every list.
    func rename(_ key: PaneKey, to label: String) async throws {
        let pane = key.pane
        try await call(key.device) { client, token in
            try await client.rename(label, pane: pane, bearer: token)
        }
    }

    /// Names a workspace; the next `home` from the computer carries the
    /// name to every list.
    func renameWorkspace(_ key: WorkspaceKey, to label: String) async throws {
        let workspace = key.workspace
        try await call(key.device) { client, token in
            try await client.renameWorkspace(workspace, to: label, bearer: token)
        }
    }

    /// Closes a workspace; the next `home` drops it from every list.
    func closeWorkspace(_ key: WorkspaceKey) async throws {
        let workspace = key.workspace
        try await call(key.device) { client, token in
            try await client.closeWorkspace(workspace, bearer: token)
        }
    }

    /// Deletes a workspace's linked worktree and closes the workspace;
    /// `discard` loses its uncommitted changes, as the person confirmed.
    func removeWorktree(_ key: WorkspaceKey, discard: Bool) async throws {
        let workspace = key.workspace
        try await call(key.device) { client, token in
            try await client.removeWorktree(workspace, discard: discard, bearer: token)
        }
    }
}
