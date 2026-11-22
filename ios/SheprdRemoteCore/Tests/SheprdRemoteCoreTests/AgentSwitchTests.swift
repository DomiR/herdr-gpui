import Foundation
@testable import SheprdRemoteCore
import Testing

/// Switching a chat to another agent it offers, as the desktop's agent
/// button does.
struct AgentSwitchTests {
    @Test func switchEncodesAsTheChatCommand() throws {
        let data = try JSONEncoder().encode(ChatCommand.setAgent("codex"))
        let json = try #require(JSONSerialization.jsonObject(with: data) as? [String: String])
        #expect(json == ["command": "set_agent", "agent": "codex"])
    }

    @Test func theSwitchShowsAtOnceOnlyForAnOfferedAgent() throws {
        let json = #"{"agent":"Claude","harnesses":{"available":["claude","codex"],"current":"claude"}}"#
        let before = try JSONDecoder().decode(Transcript.self, from: Data(json.utf8))
        let switching = try #require(Expected(.setAgent("codex")))
        #expect(!switching.shown(in: before))
        var after = before
        switching.apply(to: &after)
        #expect(after.harnesses?.current == "codex")
        #expect(switching.shown(in: after))

        var unchanged = before
        Expected.agent("gemini").apply(to: &unchanged)
        #expect(unchanged == before)
        // A newer pick replaces the older one still on its way.
        #expect(Expected.agent("codex").slot == Expected.agent("claude").slot)
    }
}
