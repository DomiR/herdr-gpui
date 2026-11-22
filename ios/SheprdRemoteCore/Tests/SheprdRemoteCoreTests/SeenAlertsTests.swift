import CryptoKit
import Foundation
@testable import SheprdRemoteCore
import Testing

@Suite struct SeenAlertsTests {
    private let start = Date(timeIntervalSince1970: 1_800_000_000)

    @Test func anEventShowsOnceAndAnotherShowsToo() {
        var seen = SeenAlerts()
        let first = seen.see("a", at: start)
        #expect(first)
        let again = seen.see("a", at: start.addingTimeInterval(5))
        #expect(!again)
        let other = seen.see("b", at: start.addingTimeInterval(6))
        #expect(other)
    }

    @Test func itKeepsOnlyTheNewestFew() {
        var seen = SeenAlerts()
        for index in 0...SeenAlerts.kept {
            let fresh = seen.see("e\(index)", at: start.addingTimeInterval(Double(index)))
            #expect(fresh)
        }
        #expect(seen.entries.count == SeenAlerts.kept)
        // The oldest went first, so it would show again.
        let oldestAgain = seen.see("e0", at: start.addingTimeInterval(1_000))
        #expect(oldestAgain)
        let newestAgain = seen.see("e\(SeenAlerts.kept)", at: start.addingTimeInterval(1_001))
        #expect(!newestAgain)
    }

    @Test func anOldSightingIsForgotten() {
        var seen = SeenAlerts()
        let early = seen.see("a", at: start)
        #expect(early)
        let late = seen.see("a", at: start.addingTimeInterval(SeenAlerts.window + 1))
        #expect(late)
    }

    @Test func theAppGroupKeepsItBetweenLaunches() throws {
        let suite = "seen-alerts-\(UUID().uuidString)"
        let defaults = try #require(UserDefaults(suiteName: suite))
        defer { defaults.removePersistentDomain(forName: suite) }
        #expect(SeenAlerts.firstSighting("a", at: start, in: defaults))
        #expect(!SeenAlerts.firstSighting("a", at: start.addingTimeInterval(1), in: defaults))
        #expect(SeenAlerts.firstSighting("b", at: start.addingTimeInterval(2), in: defaults))
        // Without the App Group nothing is remembered: every alert shows.
        #expect(SeenAlerts.firstSighting("a", at: start, in: nil))
    }
}

/// `Fixtures/notify/repeat.json`, written by `apns/tests/repeat.rs`: a
/// question, the same question sent again, and the next one for the chat,
/// as the phone receives them.
@Suite struct SeenAlertsFixtureTests {
    struct Fixture: Decodable {
        let device_private_key: String
        let pushes: [String]
        let shows: [Bool]
    }

    @Test func aCopyOfOneDecisionShowsOnce() throws {
        let url = try #require(Bundle.module.url(forResource: "Fixtures/notify/repeat", withExtension: "json"))
        let fixture = try JSONDecoder().decode(Fixture.self, from: Data(contentsOf: url))
        let key = try P256.KeyAgreement.PrivateKey(rawRepresentation: #require(Base64URL.decode(fixture.device_private_key)))
        var seen = SeenAlerts()
        var shows: [Bool] = []
        for (index, sealed) in fixture.pushes.enumerated() {
            let alert = try SealedAlert.open(sealed, device: key.publicKey) { try key.sharedSecretFromKeyAgreement(with: $0) }
            let event = try #require(alert.event)
            shows.append(seen.see(event, at: Date(timeIntervalSince1970: 1_800_000_000 + Double(index))))
        }
        #expect(shows == fixture.shows)
    }
}
