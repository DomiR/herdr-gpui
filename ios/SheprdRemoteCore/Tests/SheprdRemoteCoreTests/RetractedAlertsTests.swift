import Foundation
@testable import SheprdRemoteCore
import Testing

@Suite struct RetractedAlertsTests {
    private let start = Date(timeIntervalSince1970: 1_800_000_000)

    private func file() -> URL {
        FileManager.default.temporaryDirectory.appending(path: "retracted-\(UUID().uuidString).json")
    }

    @Test func aShownCopyOfItsEventIsRemovedOnce() {
        var retracted = RetractedAlerts()
        retracted.add("chat-w1:p2", event: "e1", at: start)
        #expect(retracted.take(shown: ["chat-w1:p2": "e1"], at: start) == ["chat-w1:p2"])
        #expect(retracted.take(shown: ["chat-w1:p2": "e1"], at: start).isEmpty)
    }

    /// The identifier is the chat's collapse id, which a newer alert for
    /// the same chat reuses: that one stays.
    @Test func aNewerAlertUnderTheSameIdentifierStays() {
        var retracted = RetractedAlerts()
        retracted.add("chat-w1:p2", event: "e1", at: start)
        retracted.delivered("chat-w1:p2", event: "e1")
        #expect(retracted.take(shown: ["chat-w1:p2": "e2"], at: start).isEmpty)
        #expect(retracted.isEmpty, "forgotten, not kept to remove later")
        retracted.add("chat-w1:p3", event: "e3", at: start)
        #expect(retracted.take(shown: ["chat-w1:p3": nil], at: start).isEmpty)
    }

    /// The extension forgets the identifier before showing another alert
    /// there, so a sweep that runs after cannot take the new one back.
    @Test func anAlertAboutToShowIsNeverTakenBack() {
        var retracted = RetractedAlerts()
        retracted.add("chat-w1:p2", event: "e1", at: start)
        retracted.delivered("chat-w1:p2", event: "e1")
        retracted.forget("chat-w1:p2")
        #expect(retracted.take(shown: ["chat-w1:p2": "e1"], at: start).isEmpty)
    }

    /// The person dismissed A; B now shows under the same identifier; a
    /// late copy of A is noted but not delivered yet when a sweep runs. B
    /// on screen does not prove A was replaced: A stays noted, and once it
    /// lands it is taken back.
    @Test func aCopyNotDeliveredYetOutlivesAnotherEventOnScreen() {
        var retracted = RetractedAlerts()
        retracted.add("chat-w1:p2", event: "A", at: start)
        #expect(retracted.take(shown: ["chat-w1:p2": "B"], at: start).isEmpty)
        retracted.forget("chat-w1:p2")
        #expect(!retracted.isEmpty, "neither a sweep nor a newer alert forgets it")
        retracted.delivered("chat-w1:p2", event: "A")
        #expect(retracted.take(shown: ["chat-w1:p2": "A"], at: start) == ["chat-w1:p2"])
    }

    /// Marking delivery names the event: a newer copy noted under the same
    /// identifier is not marked by an older one's delivery.
    @Test func deliveryMarksOnlyItsOwnEvent() {
        var retracted = RetractedAlerts()
        retracted.add("x", event: "A", at: start)
        retracted.delivered("x", event: "B")
        #expect(retracted.take(shown: ["x": "C"], at: start).isEmpty)
        #expect(!retracted.isEmpty)
    }

    /// Two late copies of one dismissed alert: the second replaces the
    /// first on screen and must be taken back too.
    @Test func aCopyReplacingALateCopyIsLateToo() {
        var retracted = RetractedAlerts()
        retracted.add("chat-w1:p2", event: "e1", at: start)
        #expect(retracted.pending("e1"))
        #expect(!retracted.pending("e2"))
        retracted.add("chat-w1:p2", event: "e1", at: start.addingTimeInterval(1))
        #expect(retracted.entries.count == 1)
        #expect(retracted.take(shown: ["chat-w1:p2": "e1"], at: start) == ["chat-w1:p2"])
    }

    @Test func aCopyNotShownYetWaitsUntilItIsOrExpires() {
        var retracted = RetractedAlerts()
        retracted.add("x", event: "e1", at: start)
        #expect(retracted.take(shown: [:], at: start).isEmpty)
        #expect(retracted.entries.count == 1)
        #expect(retracted.take(shown: ["x": "e1"], at: start.addingTimeInterval(1)) == ["x"])
        retracted.add("y", event: "e2", at: start)
        #expect(retracted.take(shown: [:], at: start.addingTimeInterval(RetractedAlerts.window + 1)).isEmpty)
        #expect(retracted.isEmpty)
    }

    @Test func itKeepsOnlyTheNewestFew() {
        var retracted = RetractedAlerts()
        for index in 0...RetractedAlerts.kept {
            retracted.add("n\(index)", event: "e\(index)", at: start)
        }
        #expect(retracted.entries.count == RetractedAlerts.kept)
        #expect(retracted.entries.first?.identifier == "n1")
    }

    @Test func theFileKeepsItBetweenUpdates() {
        let file = file()
        defer { try? FileManager.default.removeItem(at: file) }
        RetractedAlerts.update(file: file) { $0.add("x", event: "e1", at: start) }
        let removed = RetractedAlerts.update(file: file) { $0.take(shown: ["x": "e1"], at: start) }
        #expect(removed == ["x"])
        #expect(RetractedAlerts.update(file: file) { $0.isEmpty })
    }

    /// Every change is one coordinated read, change, and write: changes
    /// made at once from many threads are all kept, none overwritten.
    @Test func changesAtOnceAreAllKept() {
        let file = file()
        defer { try? FileManager.default.removeItem(at: file) }
        let count = 24
        DispatchQueue.concurrentPerform(iterations: count) { index in
            RetractedAlerts.update(file: file) { $0.add("n\(index)", event: "e\(index)", at: start) }
        }
        let kept = RetractedAlerts.update(file: file) { $0.entries.count }
        #expect(kept == count)
    }
}
