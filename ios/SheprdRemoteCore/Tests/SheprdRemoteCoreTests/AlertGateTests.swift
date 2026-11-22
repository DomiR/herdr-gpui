import Foundation
@testable import SheprdRemoteCore
import Testing

@Suite struct AlertGateTests {
    /// A gate over a fresh `SeenAlerts`, as the extension's over the App
    /// Group's.
    private static func gate() -> AlertGate {
        let seen = Seen()
        return AlertGate { seen.see($0) }
    }

    private final class Seen: @unchecked Sendable {
        private let lock = NSLock()
        private var alerts = SeenAlerts()

        func see(_ id: String) -> Bool {
            lock.withLock { alerts.see(id, at: Date()) }
        }
    }

    @Test func aFirstCopyShowsAndALaterOneRepeats() async {
        let gate = Self.gate()
        #expect(await gate.admit("a") == .first)
        await gate.delivered("a")
        #expect(await gate.admit("a") == .repeated)
        #expect(await gate.admit("b") == .first)
    }

    /// A copy arriving while the first is still prepared waits until the
    /// first was handed to iOS, so it finds the first to replace.
    @Test func anOverlappingCopyWaitsForTheFirstToBeDelivered() async {
        let gate = Self.gate()
        #expect(await gate.admit("a") == .first)
        let order = Order()
        let copy = Task {
            let admission = await gate.admit("a")
            order.note("copy admitted")
            return admission
        }
        // The copy is waiting; nothing lets it on before delivery.
        while await gate.waitingCount("a") == 0 { await Task.yield() }
        order.note("first delivered")
        await gate.delivered("a")
        #expect(await copy.value == .repeated)
        #expect(order.notes == ["first delivered", "copy admitted"])
        #expect(await gate.pending == 0)
    }

    @Test func manyOverlappingCopiesAllGoOnOnce() async {
        let gate = Self.gate()
        #expect(await gate.admit("a") == .first)
        let copies = (0..<5).map { _ in Task { await gate.admit("a") } }
        while await gate.waitingCount("a") < copies.count { await Task.yield() }
        await gate.delivered("a")
        for copy in copies { #expect(await copy.value == .repeated) }
    }

    private final class Order: @unchecked Sendable {
        private let lock = NSLock()
        private var list: [String] = []
        func note(_ step: String) { lock.withLock { list.append(step) } }
        var notes: [String] { lock.withLock { list } }
    }
}
