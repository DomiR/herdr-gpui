import AppIntents

/// Opens the app on the board, from a Control or Shortcuts. The widget
/// extension only needs the type to draw a Control; iOS runs `perform` in
/// the app, which it opens first.
struct OpenBoardIntent: AppIntent {
    static let title: LocalizedStringResource = "Open the Board"
    static let description = IntentDescription("Shows every computer's chats by what they need.")
    static let openAppWhenRun = true

    @MainActor
    func perform() async throws -> some IntentResult {
        #if !WIDGET_EXTENSION
        AppModel.current?.show(.board)
        #endif
        return .result()
    }
}

/// Opens the app on the new workspace form.
struct NewWorkspaceIntent: AppIntent {
    static let title: LocalizedStringResource = "New Workspace"
    static let description = IntentDescription("Starts a workspace with an agent on one of your computers.")
    static let openAppWhenRun = true

    @MainActor
    func perform() async throws -> some IntentResult {
        #if !WIDGET_EXTENSION
        AppModel.current?.show(.newWorkspace)
        #endif
        return .result()
    }
}
