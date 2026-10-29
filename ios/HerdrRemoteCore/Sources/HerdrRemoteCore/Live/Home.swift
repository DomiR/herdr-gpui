import Foundation

/// What one computer lists: `GET /api/v2/home` and the live socket's
/// `home` message (`crates/herdr-remote-control/src/native/model.rs`).
public struct Home: Codable, Hashable, Sendable {
    public var computer: String
    public var fingerprint: String
    /// Whether the computer's Herdr is reachable.
    public var connected: Bool
    /// Permission requests waiting across its chats: the badge.
    public var waiting: Int
    public var workspaces: [Workspace]

    public init(computer: String, fingerprint: String, connected: Bool, waiting: Int, workspaces: [Workspace]) {
        self.computer = computer
        self.fingerprint = fingerprint
        self.connected = connected
        self.waiting = waiting
        self.workspaces = workspaces
    }

    public struct Workspace: Codable, Hashable, Sendable, Identifiable {
        public var id: String
        public var label: String
        public var branch: String?
        public var cwd: String
        public var pullRequest: PullRequest?
        public var chats: [ChatRow]
        public var terminals: [TerminalRow]

        enum CodingKeys: String, CodingKey {
            case id, label, branch, cwd, chats, terminals
            case pullRequest = "pull_request"
        }
    }

    public struct PullRequest: Codable, Hashable, Sendable {
        public var number: UInt64
        public var url: String
        public var tone: Tone

        /// What the badge says, before a theme colors it.
        public enum Tone: String, Codable, Hashable, Sendable {
            case attention, held, merged, ready, waiting, other

            public init(from decoder: Decoder) throws {
                self = Self(rawValue: try decoder.singleValueContainer().decode(String.self)) ?? .other
            }
        }
    }

    public struct ChatRow: Codable, Hashable, Sendable, Identifiable {
        public var pane: String
        public var tab: String
        public var title: String
        /// Herdr's name for the agent, such as `claude`.
        public var agent: String?
        public var status: AgentStatus
        /// Absent until the daemon has linked the chat.
        public var summary: Summary?

        public var id: String { pane }
    }

    /// A chat as a row shows it, from the daemon's live transcript.
    public struct Summary: Codable, Hashable, Sendable {
        public var connected: Bool
        public var status: Status
        /// The oldest question waiting.
        public var question: Permission?
        public var questions: Int
        /// The start of the agent's latest answer, on one line.
        public var lastText: String?
        public var turnStartedMs: UInt64?
        public var usagePercent: Int?

        enum CodingKeys: String, CodingKey {
            case connected, status, question, questions
            case lastText = "last_text"
            case turnStartedMs = "turn_started_ms"
            case usagePercent = "usage_percent"
        }
    }

    public struct TerminalRow: Codable, Hashable, Sendable, Identifiable {
        public var tab: String
        public var label: String
        public var cwd: String?
        public var zoomed: Bool
        public var panes: [Pane]
        /// The agent Herdr detects in it, the most pressing first.
        public var agent: DetectedAgent?

        public var id: String { tab }

        /// The pane a phone opens first: the focused one, else the first.
        public var firstPane: Pane? { panes.first { $0.focused } ?? panes.first }

        public struct Pane: Codable, Hashable, Sendable, Identifiable {
            public var pane: String
            public var label: String?
            public var cwd: String?
            public var focused: Bool
            public var id: String { pane }
        }

        public struct DetectedAgent: Codable, Hashable, Sendable {
            public var name: String?
            public var status: AgentStatus
        }
    }

    /// Herdr's own status for a pane's agent.
    public enum AgentStatus: String, Codable, Hashable, Sendable {
        case unknown, idle, working, blocked, done

        public init(from decoder: Decoder) throws {
            self = Self(rawValue: try decoder.singleValueContainer().decode(String.self)) ?? .unknown
        }
    }
}
