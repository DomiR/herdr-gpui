import Foundation
import Testing
@testable import SheprdRemoteCore

/// What keeps a phone's fleet actions whole when things go wrong: keys of
/// links whose reply never came, and a phone a computer no longer counts
/// as a member.
struct FleetRecoveryTests {
    typealias Computer = FleetClientTests.Computer

    static func pending(_ host: String, seed: UInt8 = 1) -> FleetPendingLink {
        FleetPendingLink(
            host: host, name: host, keyKind: .software, keyData: Data(repeating: seed, count: 32),
            publicKey: "k\(seed)", point: Data(repeating: seed, count: 65)
        )
    }

    @Test func aLinkWithNoReplyKeepsItsKeyUntilTheFleetSays() {
        let pending = [Self.pending("aa"), Self.pending("bb")]
        // No answer: nothing is known yet, so both wait.
        let unknown = FleetPendingLink.settle(pending, linked: [:])
        #expect(unknown.done.isEmpty && unknown.waiting == pending)
        // The fleet later shows one linked: its key becomes a computer, the
        // other still waits.
        let later = FleetPendingLink.settle(pending, linked: ["aa": pending[0].point])
        #expect(later.done == [pending[0]] && later.waiting == [pending[1]])
        // Refused by the computer that took it, and missing from its own
        // read: that one goes, the other stays.
        let refused = FleetPendingLink.settle(pending, linked: ["aa": pending[0].point], refused: ["aa", "BB"])
        #expect(refused.done == [pending[0]] && refused.waiting.isEmpty)
    }

    @Test func aHostLinkedWithAnotherKeyIsNotThisKeysLink() {
        // A first post's answer was lost; its key waits. The fleet shows the
        // host linked, but with another key: an earlier link, not this one.
        let first = Self.pending("aa", seed: 1)
        let other = Self.pending("aa", seed: 9)
        let earlier = FleetPendingLink.settle([first], linked: ["aa": other.point])
        #expect(earlier.done.isEmpty && earlier.waiting == [first])
        // Only the key made for it here makes it done.
        let landed = FleetPendingLink.settle([first], linked: ["aa": first.point])
        #expect(landed.done == [first] && landed.waiting.isEmpty)
    }

    @Test func aLinkWhoseComputerWasNotSavedKeepsItsKey() {
        let done = [Self.pending("aa", seed: 1), Self.pending("bb", seed: 2)]
        // The phone could not save the computer for "bb" (a Keychain write
        // failed): its key stays pending for the next settle.
        var kept: [String] = []
        let unsaved = FleetPendingLink.unsaved(done) { link in
            kept.append(link.host)
            return link.host != "bb"
        }
        #expect(kept == ["aa", "bb"])
        #expect(unsaved == [done[1]])
        // Every save landed: nothing stays.
        #expect(FleetPendingLink.unsaved(done) { _ in true }.isEmpty)
    }

    @Test func addingAgainReusesTheKeyStillWaiting() {
        let waiting = [Self.pending("aa", seed: 1), Self.pending("bb", seed: 2)]
        #expect(FleetPendingLink.waiting(for: "AA", in: waiting) == waiting[0])
        #expect(FleetPendingLink.waiting(for: "cc", in: waiting) == nil)
    }

    @Test func aKeyMadeAgainReplacesTheOneWaiting() throws {
        let merged = FleetPendingLink.merge([Self.pending("aa"), Self.pending("bb")], [Self.pending("AA", seed: 2)])
        #expect(merged.map(\.host) == ["bb", "AA"])
        #expect(merged.last?.keyData == Data(repeating: 2, count: 32))
        let record = Self.pending("aa")
        let decoded = try? JSONDecoder().decode(FleetPendingLink.self, from: JSONEncoder().encode(record))
        #expect(decoded == record)
    }

    @Test func aComputerThatNoLongerCountsThePhoneSaysSo() throws {
        let fleet = String(repeating: "09", count: 32)
        let phone = String(repeating: "bb", count: 32)
        let removed = try JSONDecoder().decode(API.FleetStatus.self, from: FleetClientTests.status(next: nil))
        #expect(removed.lacks(member: phone, in: fleet))
        // Another fleet's computer says nothing about this one.
        #expect(!removed.lacks(member: phone, in: String(repeating: "08", count: 32)))
        // Still a member, or waiting to join: not lacking.
        let next = API.FleetNext(fleet: fleet, seq: 1, parents: [])
        let member = try JSONDecoder().decode(API.FleetStatus.self, from: FleetClientTests.status(next: next))
        #expect(!member.lacks(member: phone, in: fleet))
        let pending = API.FleetStatus(view: removed.view, join: .pending)
        #expect(!pending.lacks(member: phone, in: fleet))
    }
}
