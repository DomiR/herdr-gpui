#if DEBUG
import Foundation
import SheprdRemoteCore

// A made-up chat for `-sampleChat activity`: a turn still running with
// sub-agents, background shells, a monitor, a plan, changed files, and
// pull requests, so
// screenshots of the activity panel show no one's real work.

extension PaneKey {
    static let activitySample = PaneKey(device: "sample", pane: "activity-sample")
}

extension Transcript {
    static var activitySample: Transcript {
        let now = UInt64(Date.now.timeIntervalSince1970 * 1000)
        let ago = { (seconds: UInt64) in now - seconds * 1000 }
        var transcript = Transcript(agent: "Claude Code")
        transcript.status = .working
        transcript.turn = Turn(startedMs: ago(184))
        transcript.entries = [
            Entry(id: 1, kind: .user(text: "Show the chat's right rail on iOS: sub-agents, shells, and the plan.", files: 0)),
            Entry(id: 2, kind: .agent(text: "I'll split this up: one sub-agent maps the desktop rail while another reads the iOS chat screen.")),
        ] + [
            Self.sampleEdit(3, path: "ios/SheprdRemote/Views/ChatScreen.swift",
                            old: "if let usage = store?.transcript?.usage {",
                            new: "if transcript.hasActivityPanel {\n    AgentPanelButton(transcript: transcript)\n}\nif let usage = store?.transcript?.usage {"),
            Self.sampleEdit(4, path: "ios/SheprdRemoteCore/Sources/SheprdRemoteCore/Chat/Change.swift",
                            old: "case autoAccept(Bool)",
                            new: "case autoAccept(Bool)\ncase activity(AgentActivity)"),
        ].compactMap(\.self)
        transcript.activities = [
            AgentActivity(callId: "a1", kind: .subagent, label: "Map the desktop rail's sections",
                     startedMs: ago(142), detail: "Reading chat/view/rail/activity.rs", stoppable: true),
            AgentActivity(callId: "a2", kind: .subagent, label: "Find where the iOS chat header adds buttons",
                     startedMs: ago(61), detail: "Searching for ToolbarItem in ios/SheprdRemote", stoppable: true),
            AgentActivity(callId: "a3", kind: .subagent, label: "Check how the phone decodes changes",
                     startedMs: ago(300), state: .finished, finishedMs: ago(212)),
            AgentActivity(callId: "s1", kind: .shell, label: "swift test --filter ChatActivitiesTests",
                     startedMs: ago(38), detail: "Building for debugging…", stoppable: true),
            AgentActivity(callId: "s2", kind: .shell, label: "cargo test -p sheprd-remote-control",
                     startedMs: ago(260), state: .failed, finishedMs: ago(231)),
            AgentActivity(callId: "s3", kind: .shell, label: "npm run dev",
                     startedMs: ago(900), state: .killed, finishedMs: ago(400)),
            AgentActivity(callId: "m1", kind: .monitor, label: "CI on pull request #1442",
                     startedMs: ago(600), detail: "3 of 7 checks passed"),
        ]
        transcript.plan = [
            ("Decode activity changes on the phone", PlanStatus.completed),
            ("Group activities like the desktop rail", .completed),
            ("Add the header button and the panel", .inProgress),
            ("Stop a running sub-agent from the phone", .pending),
            ("Screenshot it in the simulator", .pending),
        ].compactMap { content, status in
            try? JSONDecoder().decode(PlanEntry.self, from: Data(#"{"content":"\#(content)","status":"\#(status.rawValue)"}"#.utf8))
        }
        transcript.pullRequests = [
            ChatPullRequest(repo: "example/forecast", number: 412, actions: [.created, .pushed]),
            ChatPullRequest(repo: "example/forecast", number: 398, actions: [.reviewed, .merged]),
            ChatPullRequest(repo: "example/charts", number: 57),
        ]
        return transcript
    }

    private static func sampleEdit(_ id: UInt64, path: String, old: String, new: String) -> Entry? {
        let content: [String: Any] = ["type": "diff", "path": path, "old": old, "new": new]
        let kind: [String: Any] = ["type": "tool", "call_id": "e\(id)", "title": "Edit", "kind": "edit", "status": "completed", "content": [content]]
        guard let data = try? JSONSerialization.data(withJSONObject: ["id": id, "kind": kind]) else { return nil }
        return try? JSONDecoder().decode(Entry.self, from: data)
    }
}
#endif
