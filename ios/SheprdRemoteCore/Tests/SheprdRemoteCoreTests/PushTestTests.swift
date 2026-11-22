import CryptoKit
import Foundation
import Testing
@testable import SheprdRemoteCore

/// Settings' test push: the daemon's sealed test alert
/// (`Fixtures/notify/test.json`, written by `apns/tests/test_push.rs`)
/// opens to its id, and the receipt the extension leaves can be read back.
@Suite struct PushTestTests {
    struct Fixture: Decodable {
        let device_private_key: String
        let apns: APNs
        let plaintext: Plaintext
        struct APNs: Decodable { let sealed: String }
        struct Plaintext: Decodable { let test: String }
    }

    @Test func theDaemonsTestPushOpensToItsId() throws {
        let url = try #require(Bundle.module.url(forResource: "Fixtures/notify/test", withExtension: "json"))
        let fixture = try JSONDecoder().decode(Fixture.self, from: Data(contentsOf: url))
        let key = try P256.KeyAgreement.PrivateKey(rawRepresentation: #require(Base64URL.decode(fixture.device_private_key)))
        let alert = try SealedAlert.open(fixture.apns.sealed, device: key.publicKey) { try key.sharedSecretFromKeyAgreement(with: $0) }
        #expect(alert.kind == .test)
        #expect(alert.test == fixture.plaintext.test)
        #expect(alert.presentation.actions.isEmpty)
    }

    @Test func otherAlertsCarryNoTestId() throws {
        let alert = try JSONDecoder().decode(
            SealedAlert.self,
            from: Data(#"{"kind":"turn_ended","computer":"AB","pane":"w1:p1"}"#.utf8)
        )
        #expect(alert.test == nil)
    }

    @Test func aReceiptIsReadBackAndOnlyTheNewestAreKept() throws {
        let suite = "PushTestTests-\(UUID().uuidString)"
        let defaults = try #require(UserDefaults(suiteName: suite))
        defer { defaults.removePersistentDomain(forName: suite) }
        #expect(PushTestReceipts.arrival(of: "a", in: defaults) == nil)
        let start = Date(timeIntervalSince1970: 1_000_000)
        PushTestReceipts.record("a", at: start, in: defaults)
        #expect(PushTestReceipts.arrival(of: "a", in: defaults) == start)
        for n in 1...PushTestReceipts.kept {
            PushTestReceipts.record("n\(n)", at: start.addingTimeInterval(Double(n)), in: defaults)
        }
        #expect(PushTestReceipts.arrival(of: "a", in: defaults) == nil)
        #expect(PushTestReceipts.arrival(of: "n\(PushTestReceipts.kept)", in: defaults) != nil)
        // Without the App Group, nothing is recorded and nothing breaks.
        PushTestReceipts.record("b", at: start, in: nil)
        #expect(PushTestReceipts.arrival(of: "b", in: nil) == nil)
    }
}
