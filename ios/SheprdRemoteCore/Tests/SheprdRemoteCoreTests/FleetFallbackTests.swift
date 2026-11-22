import Foundation
import Synchronization
import Testing
@testable import SheprdRemoteCore

/// Keeping what a lost answer carries, calling through the live link, and
/// keeping the relay keys a linked computer already works with.
struct FleetFallbackTests {
    struct Offline: Error, Equatable { let name: String }

    @Test func aLaggingComputersReadDropsNoKeys() {
        // A link taken by a computer that lags (it holds it waiting for its
        // parents) is missing from that computer's read, and from any
        // other that has not caught up: absence alone drops nothing.
        let pending = [FleetRecoveryTests.pending("aa"), FleetRecoveryTests.pending("bb")]
        let lagging = FleetPendingLink.settle(pending, linked: [:])
        #expect(lagging.done.isEmpty && lagging.waiting == pending)
        // Only an explicit refusal of a link drops it, and only that one.
        let refused = FleetPendingLink.settle(pending, linked: [:], refused: ["bb"])
        #expect(refused.waiting == [pending[0]])
    }

    @Test func fleetCallsGoThroughTheLiveConnectionWhenThereIsOne() async throws {
        typealias Computer = FleetClientTests.Computer
        // The live link (a relay, say) answers; the saved address is the
        // network the phone left.
        let live = Computer { _, _, _ in FleetClientTests.status(next: nil) }
        let saved = Computer { _, _, _ in throw Offline(name: "lan") }
        let online = Mutex(true)
        let client = FleetClient.preferring(
            live: { online.withLock { $0 } ? live.send : nil },
            fallback: saved.send
        )
        let status = try await client.status(member: nil)
        #expect(status.view != nil)
        #expect(live.requests.withLock { $0.count } == 1)
        #expect(saved.requests.withLock { $0.isEmpty })
        // With no live link (a computer just paired), the saved addresses.
        online.withLock { $0 = false }
        await #expect(throws: Offline(name: "lan")) { try await client.status(member: nil) }
        #expect(saved.requests.withLock { $0.count } == 1)
        #expect(live.requests.withLock { $0.count } == 1)
    }

    @Test func aLostOrFailedAnswerKeepsWhatTheRequestCarried() {
        // No answer, or the computer failed: the join may be waiting there.
        #expect(fleetRequestMayHaveLanded(status: nil))
        #expect(fleetRequestMayHaveLanded(status: 500))
        #expect(fleetRequestMayHaveLanded(status: 503))
        // Refused: nothing landed, the keys can go.
        for refused in [400, 401, 403, 404, 409, 429] {
            #expect(!fleetRequestMayHaveLanded(status: refused))
        }
    }

    @Test func aRefreshedRouteKeepsTheKeysAlreadyInUse() throws {
        let host = FleetRouteTests.Host()
        let id = host.id.hexString
        let bundle = RelayKeyPair()
        let first = try #require(FleetRouteTests.status([try host.route(host.record(rev: 1))]).relayRecord(for: id, keys: bundle))
        // The phone since sent keys of its own for this computer.
        let own = RelayKeyPair()
        var saved = first
        saved.dialKey = own.dialKey
        saved.tunnelKey = own.tunnelKey
        let refreshed = try #require(
            FleetRouteTests.status([try host.route(host.record(rev: 2))]).relayRecord(for: id, keys: bundle, replacing: saved)
        )
        #expect(refreshed.dialKey == own.dialKey && refreshed.tunnelKey == own.tunnelKey)
        #expect(refreshed.signedRev == 2)
    }
}
