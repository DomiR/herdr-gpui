import Foundation

// Work the agent started that runs beside the conversation, as
// `sheprd_chat_protocol::Activity` writes it: sub-agents, background shells,
// and monitors, which can outlive the turn that started them. Which agents
// report them, and which can stop them, is the chat program's to say.

/// One sub-agent, background shell, or monitor, from the tool call that
/// started it.
public struct AgentActivity: Codable, Hashable, Sendable, Identifiable {
    /// The tool call that started it; unique among a chat's activities.
    public var callId: String
    /// What it is doing now, such as a sub-agent's latest step.
    public var detail: String?
    /// When it ended (Unix milliseconds), once it has.
    public var finishedMs: UInt64?
    public var kind: AgentActivityKind
    /// What it is for: a sub-agent's description or a shell's command.
    public var label: String
    public var startedMs: UInt64
    public var state: AgentActivityState
    /// The agent can end it on request (`ChatCommand.stopActivity`).
    public var stoppable: Bool

    public var id: String { callId }
    public var isRunning: Bool { state == .running }

    /// The most a transcript keeps; a new one replaces the oldest ended.
    public static let limit = 32
    /// The longest call id, label, or detail, in UTF-8 bytes.
    public static let textLimit = 1024

    public init(
        callId: String, kind: AgentActivityKind, label: String, startedMs: UInt64,
        state: AgentActivityState = .running, detail: String? = nil, finishedMs: UInt64? = nil, stoppable: Bool = false
    ) {
        self.callId = callId
        self.detail = detail
        self.finishedMs = finishedMs
        self.kind = kind
        self.label = label
        self.startedMs = startedMs
        self.state = state
        self.stoppable = stoppable
    }

    enum CodingKeys: String, CodingKey {
        case detail, kind, label, state, stoppable
        case callId = "call_id"
        case finishedMs = "finished_ms"
        case startedMs = "started_ms"
    }

    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        callId = try c.decode(String.self, forKey: .callId)
        detail = try c.decodeIfPresent(String.self, forKey: .detail)
        finishedMs = try c.decodeIfPresent(UInt64.self, forKey: .finishedMs)
        kind = try c.decodeIfPresent(AgentActivityKind.self, forKey: .kind) ?? .other
        label = try c.decode(String.self, forKey: .label)
        startedMs = try c.decode(UInt64.self, forKey: .startedMs)
        state = try c.decodeIfPresent(AgentActivityState.self, forKey: .state) ?? .running
        stoppable = try c.decodeIfPresent(Bool.self, forKey: .stoppable) ?? false
    }

    public func encode(to encoder: Encoder) throws {
        var c = encoder.container(keyedBy: CodingKeys.self)
        try c.encode(callId, forKey: .callId)
        try c.encodeIfPresent(detail, forKey: .detail)
        try c.encodeIfPresent(finishedMs, forKey: .finishedMs)
        try c.encode(kind, forKey: .kind)
        try c.encode(label, forKey: .label)
        try c.encode(startedMs, forKey: .startedMs)
        try c.encode(state, forKey: .state)
        if stoppable { try c.encode(true, forKey: .stoppable) }
    }

    /// Whether every string fits `textLimit`, as the Rust reducer checks.
    var fits: Bool {
        [callId, label, detail].compactMap { $0 }.allSatisfy { $0.utf8.count <= Self.textLimit }
    }
}

public enum AgentActivityKind: String, Codable, Hashable, Sendable, CaseIterable {
    /// Another agent the agent delegated a task to.
    case subagent
    /// A command left running in the background.
    case shell
    /// Something watched in the background, such as Claude Code's `Monitor`.
    case monitor
    /// A kind from a newer program.
    case other

    public init(from decoder: Decoder) throws {
        self = Self(rawValue: try decoder.singleValueContainer().decode(String.self)) ?? .other
    }
}

public enum AgentActivityState: String, Codable, Hashable, Sendable {
    case running
    case failed
    /// Stopped before it ended: by the agent, the person, or the agent exiting.
    case killed
    /// Ended on its own; also an outcome from a newer program.
    case finished

    public init(from decoder: Decoder) throws {
        self = Self(rawValue: try decoder.singleValueContainer().decode(String.self)) ?? .finished
    }
}

/// The agents a chat may switch to, and the one it runs now
/// (`sheprd_chat_protocol::Harnesses`), by name.
public struct Harnesses: Codable, Hashable, Sendable {
    public var available: [String]
    public var current: String?

    enum CodingKeys: String, CodingKey { case available, current }

    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        available = try c.decodeIfPresent([String].self, forKey: .available) ?? []
        current = try c.decodeIfPresent(String.self, forKey: .current)
    }
}

/// One section of the chat's activity panel: what runs, in the order it
/// started, then what ended, most recent first.
public struct AgentActivityGroup: Hashable, Sendable {
    public var running: [AgentActivity]
    public var ended: [AgentActivity]

    public var isEmpty: Bool { running.isEmpty && ended.isEmpty }
}

extension AgentActivityKind {
    /// The panel's section for this kind: a kind from a newer program is
    /// listed with the shells, as something left running.
    public var section: AgentActivityKind { self == .other ? .shell : self }
}

extension Transcript {
    /// Replaces the activity with the same call id, or adds it, making room
    /// by dropping the oldest that ended: the Rust reducer's `record`.
    mutating func record(_ activity: AgentActivity) throws(ChangeError) {
        guard activity.fits else { throw .activityText }
        if let index = activities.firstIndex(where: { $0.callId == activity.callId }) {
            activities[index] = activity
            return
        }
        if activities.count >= AgentActivity.limit {
            guard let ended = activities.firstIndex(where: { !$0.isRunning }) else { throw .activityLimit }
            activities.remove(at: ended)
        }
        activities.append(activity)
    }

    /// The activities the panel lists under `section`'s kind.
    public func activityGroup(_ section: AgentActivityKind) -> AgentActivityGroup {
        let mine = activities.filter { $0.kind.section == section.section }
        let ended = mine.filter { !$0.isRunning }.sorted { ($0.finishedMs ?? 0) > ($1.finishedMs ?? 0) }
        return AgentActivityGroup(running: mine.filter(\.isRunning), ended: ended)
    }

    /// How many activities still run.
    public var runningActivities: Int { activities.count { $0.isRunning } }

    /// The files the agent's tool calls changed, each with its latest
    /// diff, in the order first changed.
    public var changedFiles: [ChangedText] {
        var order: [String] = []
        var latest: [String: ChangedText] = [:]
        for entry in entries {
            guard case let .tool(tool) = entry.kind else { continue }
            for case let .diff(path, old, new) in tool.content {
                if latest[path] == nil { order.append(path) }
                latest[path] = ChangedText(path: path, old: old, new: new)
            }
        }
        return order.compactMap { latest[$0] }
    }

    /// Whether the activity panel has anything to show.
    public var hasActivityPanel: Bool {
        !activities.isEmpty || !plan.isEmpty || !changedFiles.isEmpty || !pullRequests.isEmpty
    }
}

/// A file's text before and after a tool call changed it.
public struct ChangedText: Hashable, Sendable, Identifiable {
    public var path: String
    public var old: String?
    public var new: String
    public var id: String { path }
}
