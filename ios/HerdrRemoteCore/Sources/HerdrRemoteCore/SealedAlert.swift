import CryptoKit
import Foundation

/// What a sealed APNs alert holds once the Notification Service Extension
/// opens it (`NotificationSeal`): the daemon writes the same keys
/// (`crates/herdr-remote-control/src/apns`), and `Fixtures/notify` holds
/// both sides to them. APNs itself only sees "Herdr", "New activity", and
/// ciphertext.
public struct SealedAlert: Codable, Hashable, Sendable {
    public enum Kind: String, Codable, Hashable, Sendable {
        case question
        case turnEnded = "turn_ended"
        case error
        case other

        public init(from decoder: Decoder) throws {
            self = Self(rawValue: try decoder.singleValueContainer().decode(String.self)) ?? .other
        }
    }

    public struct Permission: Codable, Hashable, Sendable {
        public var id: PermissionId
        public var options: [ChatActivityState.Option]
    }

    public var kind: Kind
    /// Closes the chat's notifications instead of showing one.
    public var clear: Bool
    /// The computer's CA fingerprint, 64 uppercase hex digits.
    public var computer: String
    public var pane: String
    public var title: String
    public var body: String
    public var badge: Int?
    public var permission: Permission?

    enum CodingKeys: String, CodingKey { case kind, clear, computer, pane, title, body, badge, permission }

    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        kind = try c.decodeIfPresent(Kind.self, forKey: .kind) ?? .other
        clear = try c.decodeIfPresent(Bool.self, forKey: .clear) ?? false
        computer = try c.decode(String.self, forKey: .computer)
        pane = try c.decode(String.self, forKey: .pane)
        title = try c.decodeIfPresent(String.self, forKey: .title) ?? ""
        body = try c.decodeIfPresent(String.self, forKey: .body) ?? ""
        badge = try c.decodeIfPresent(Int.self, forKey: .badge)
        permission = try c.decodeIfPresent(Permission.self, forKey: .permission)
    }

    /// Opens an APNs payload's `sealed` field with the device's key.
    public static func open(
        _ sealed: String,
        device: P256.KeyAgreement.PublicKey,
        agree: (P256.KeyAgreement.PublicKey) throws -> SharedSecret
    ) throws -> SealedAlert {
        guard let bytes = Base64URL.decode(sealed) else { throw NotificationSeal.Failure.format }
        let plaintext = try NotificationSeal.open(bytes, device: device, agree: agree)
        return try JSONDecoder().decode(SealedAlert.self, from: plaintext)
    }

    /// The notification's category and its actions.
    public var presentation: AlertPresentation { AlertPresentation(self) }
}

/// The category a notification shows its actions under. iOS ties actions
/// to categories, so each distinct set of answers gets its own category,
/// named by a digest of the answers, which the extension registers before
/// it hands the notification over.
public struct AlertPresentation: Hashable, Sendable {
    public struct Action: Hashable, Sendable {
        public enum Style: Hashable, Sendable {
            case plain
            /// Shown in red.
            case destructive
            /// Asks for Face ID or the passcode first: "always" answers.
            case authenticated
            /// A text field; its text becomes the next prompt.
            case textInput(placeholder: String)
        }

        public var id: String
        public var title: String
        public var style: Style
    }

    public var category: String?
    public var actions: [Action]

    /// Answers from a notification, at most four as iOS shows.
    public static let maxAnswers = 4
    static let answerPrefix = "answer:"
    static let replyAction = "reply"
    public static let finishedCategory = "finished"

    /// A finished turn's Reply, which the app registers at launch.
    public static let finished = AlertPresentation(
        category: finishedCategory,
        actions: [Action(id: replyAction, title: "Reply", style: .textInput(placeholder: "Next prompt"))]
    )

    init(category: String?, actions: [Action]) {
        self.category = category
        self.actions = actions
    }

    init(_ alert: SealedAlert) {
        switch alert.kind {
        case .question:
            let options = Array((alert.permission?.options ?? []).prefix(Self.maxAnswers))
            guard !options.isEmpty else {
                category = nil
                actions = []
                return
            }
            actions = options.map { option in
                let style: Action.Style = switch option.kind {
                case .allowAlways, .rejectAlways: .authenticated
                case .rejectOnce: .destructive
                default: .plain
                }
                return Action(id: Self.answerPrefix + option.id, title: option.name, style: style)
            }
            let digest = SHA256.hash(data: Data(options.map { "\($0.id)\u{1f}\($0.name)\u{1f}\($0.kind.rawValue)" }.joined(separator: "\u{1e}").utf8))
            category = "question." + digest.prefix(8).map { String(format: "%02x", $0) }.joined()
        case .turnEnded:
            self = Self.finished
        case .error, .other:
            category = nil
            actions = []
        }
    }

    /// The command a tapped action sends, given the notification's
    /// permission and, for Reply, the text typed.
    public static func command(action: String, permission: PermissionId?, text: String?) -> ChatCommand? {
        if action.hasPrefix(answerPrefix), let permission {
            let option = String(action.dropFirst(answerPrefix.count))
            return option.isEmpty ? nil : .answer(permission: permission, option: option)
        }
        if action == replyAction, let text = text?.trimmingCharacters(in: .whitespacesAndNewlines), !text.isEmpty {
            return .prompt(text: text)
        }
        return nil
    }
}

/// Checks a Live Activity's answer against the chat it names. Every paired
/// computer holds the push-to-start token, so any of them can start an
/// activity that names another computer's chat; answering only when that
/// chat really waits on the same question, with the same answer offered,
/// keeps one computer from steering an answer to another.
public enum ActivityAnswer {
    /// Whether `permission` in `transcript` is the question the activity
    /// showed as `question` (the daemon's excerpt of its title: whitespace
    /// collapsed, cut with an ellipsis) and offers `option`.
    public static func matches(question: String, permission: PermissionId, option: String, in transcript: Transcript) -> Bool {
        guard let waiting = transcript.permissions.first(where: { $0.id == permission }),
              waiting.options.contains(where: { $0.id == option })
        else { return false }
        let title = waiting.title.split(whereSeparator: \.isWhitespace).joined(separator: " ")
        if question.hasSuffix("\u{2026}") {
            let shown = question.dropLast()
            return !shown.isEmpty && title.hasPrefix(shown) && title.count > shown.count
        }
        return title == question
    }
}
