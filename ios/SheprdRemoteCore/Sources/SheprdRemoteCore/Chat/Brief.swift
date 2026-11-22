import Foundation

/// A prompt another agent wrote, such as the brief an agent starts a chat
/// with (`sheprd chat new --prompt`): `sheprd_chat_protocol::Brief`. The
/// person did not type it, so the chat shows it as a document to read.
public struct Brief: Codable, Hashable, Sendable {
    /// The most briefs a transcript keeps (`MAX_BRIEFS`).
    public static let limit = 16

    public var entry: EntryId
    /// The agent that wrote it, by name (`claude`), when known.
    public var from: String?

    public init(entry: EntryId, from: String? = nil) {
        self.entry = entry
        self.from = from
    }

    /// The writer's name as Sheprd shows it (`Agent::display_name`).
    public var fromName: String? {
        guard let from, !from.isEmpty else { return nil }
        return Self.names[from] ?? from.capitalized
    }

    private static let names = [
        "claude": "Claude", "codex": "Codex", "copilot": "Copilot", "gemini": "Gemini",
        "kimi": "Kimi Code", "opencode": "OpenCode", "pi": "Pi",
    ]
}

extension Transcript {
    /// The brief `id` is, if another agent wrote it.
    public func brief(_ id: EntryId) -> Brief? { briefs.first { $0.entry == id } }

    /// Marks a user entry as a brief; marking one again replaces its sender.
    mutating func add(_ brief: Brief) throws(ChangeError) {
        guard case .user = entry(brief.entry)?.kind else { throw .notPrompt }
        if let index = briefs.firstIndex(where: { $0.entry == brief.entry }) {
            briefs[index] = brief
            return
        }
        guard briefs.count < Brief.limit else { throw .briefLimit }
        briefs.append(brief)
    }
}
