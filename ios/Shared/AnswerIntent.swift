import AppIntents
import HerdrRemoteCore

/// Answers a chat's question from its Live Activity without opening the
/// app. iOS runs a `LiveActivityIntent` in the app's process, which signs
/// in to the computer with this phone's device key and sends the answer;
/// the widget extension only needs the type to draw the button.
struct AnswerIntent: LiveActivityIntent {
    static let title: LocalizedStringResource = "Answer a Question"
    static let isDiscoverable = false

    /// The computer's CA fingerprint, as the activity carries it.
    @Parameter(title: "Computer") var computer: String
    @Parameter(title: "Chat") var pane: String
    @Parameter(title: "Question") var permission: Int
    @Parameter(title: "Answer") var option: String
    /// The question as the activity showed it, which must still be waiting
    /// on that chat (`ActivityAnswer`).
    @Parameter(title: "Question text") var question: String

    init() {}

    init(computer: String, pane: String, permission: PermissionId, option: String, question: String) {
        self.computer = computer
        self.pane = pane
        self.permission = Int(clamping: permission.value)
        self.option = option
        self.question = question
    }

    func perform() async throws -> some IntentResult {
        #if !WIDGET_EXTENSION
        try await BackgroundCommands.answerFromActivity(
            computer: computer,
            pane: pane,
            permission: PermissionId(UInt64(max(0, permission))),
            option: option,
            question: question
        )
        #endif
        return .result()
    }
}
