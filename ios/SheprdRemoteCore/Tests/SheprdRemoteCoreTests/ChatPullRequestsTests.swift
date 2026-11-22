import Foundation
import Testing
@testable import SheprdRemoteCore

/// The pull requests a chat's agent worked on, on the phone: decoded as the
/// daemon writes them and reduced as the Rust reducer does, so the activity
/// panel lists what the desktop rail does.
struct ChatPullRequestsTests {
    @Test func decodesTheDaemonsPullRequestChanges() throws {
        guard case let .chatChanges(pane, changes) = try WireTests.event("chat_pull_requests") else {
            Issue.record("not chat changes")
            return
        }
        #expect(pane == "p1")
        var transcript = Transcript(agent: "Claude")
        for step in changes { try transcript.apply(step.change) }
        // The third names the first again, in another case.
        #expect(transcript.pullRequests.map(\.number) == [1490, 1489])
        #expect(transcript.pullRequests[0].actions == [.created, .pushed])
        #expect(transcript.pullRequests[0].url == "https://github.com/sheprd-labs/sheprd/pull/1490")
        #expect(transcript.pullRequests[1].actions.isEmpty)
        #expect(transcript.hasActivityPanel)
    }

    @Test func aSnapshotCarriesItsPullRequests() throws {
        let json = #"{"agent":"Claude","pull_requests":[{"number":7,"repo":"o/r","url":"https://github.com/o/r/pull/7","actions":["merged","teleported"]}]}"#
        let transcript = try JSONDecoder().decode(Transcript.self, from: Data(json.utf8))
        #expect(transcript.pullRequests.first?.actions == [.merged, .other])
        #expect(transcript.pullRequests.first?.ownerAndName?.owner == "o")
    }

    @Test func refusesWhatTheReducerRefuses() {
        var transcript = Transcript(agent: "Claude")
        #expect(throws: ChangeError.pullRequest) { try transcript.apply(.pullRequest(ChatPullRequest(repo: "norepo", number: 1))) }
        #expect(throws: ChangeError.pullRequest) {
            try transcript.apply(.pullRequest(ChatPullRequest(repo: "o/r", number: 1, actions: [.created, .created])))
        }
        for number in 1...UInt64(ChatPullRequest.limit) {
            try? transcript.apply(.pullRequest(ChatPullRequest(repo: "o/r", number: number)))
        }
        #expect(throws: ChangeError.pullRequestLimit) {
            try transcript.apply(.pullRequest(ChatPullRequest(repo: "o/r", number: 999)))
        }
        // One already kept still takes new actions.
        #expect(throws: Never.self) { try transcript.apply(.pullRequest(ChatPullRequest(repo: "o/r", number: 1, actions: [.merged]))) }
        #expect(transcript.pullRequests[0].actions == [.merged])
    }
}
