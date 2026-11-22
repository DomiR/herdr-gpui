import CryptoKit
import Foundation
import Testing
@testable import SheprdRemoteCore

/// Handles the phone makes for the push relay, against the bytes
/// `crates/sheprd-push-relay` opens (`Fixtures/push_relay/handle.json`).
@Suite struct PushRelayHandleTests {
    static func fixture() throws -> [String: String] {
        let url = try #require(Bundle.module.url(forResource: "Fixtures/push_relay/handle", withExtension: "json"))
        return try #require(JSONSerialization.jsonObject(with: Data(contentsOf: url)) as? [String: String])
    }

    static func bytes(_ fixture: [String: String], _ field: String) throws -> Data {
        let text = try #require(fixture[field])
        return try #require(Base64URL.decode(text))
    }

    static func credential(_ fixture: [String: String], expiresS: UInt64 = 2_000_000_000) throws -> PushRelayCredential {
        let issued = PushRelayIssued(
            credential: fixture["credential"] ?? "",
            expiresS: expiresS,
            relayKeyId: fixture["relay_key_id"] ?? "",
            relayKey: fixture["relay_key"] ?? ""
        )
        return try PushRelayCredential(issued, environment: .sandbox)
    }

    @Test func makesTheHandleThePushRelayOpensByteForByte() throws {
        let fixture = try Self.fixture()
        let token = try #require(fixture["token"])
        let id = try #require(fixture["id"])
        let handle = try Self.credential(fixture).handle(
            token: token,
            kind: .activity,
            id: id,
            ephemeral: P256.KeyAgreement.PrivateKey(rawRepresentation: Self.bytes(fixture, "ephemeral_private_key")),
            nonce: AES.GCM.Nonce(data: Self.bytes(fixture, "nonce"))
        )
        #expect(handle == fixture["handle"])
    }

    @Test func thePushRelaysKeyOpensWhatThePhoneSealed() throws {
        let fixture = try Self.fixture()
        let relay = try P256.KeyAgreement.PrivateKey(rawRepresentation: Self.bytes(fixture, "relay_private_key"))
        let token = try #require(fixture["token"])
        let handle = try Self.credential(fixture).handle(token: token.uppercased(), kind: .pushToStart)
        let parts = handle.split(separator: ".", maxSplits: 1).map(String.init)
        #expect(parts.first == fixture["relay_key_id"])
        let sealed = try #require(Base64URL.decode(parts[1]))
        let opened = try NotificationSeal.open(
            sealed, device: relay.publicKey, info: PushRelayKind.handleInfo, agree: relay.sharedSecretFromKeyAgreement
        )
        let json = try #require(JSONSerialization.jsonObject(with: opened) as? [String: String])
        #expect(json["kind"] == "push_to_start")
        #expect(json["token"] == fixture["token"])
        #expect(json["credential"] == fixture["credential"])
        let id = try #require(json["id"])
        #expect(id.count == 32 && id.allSatisfy { "0123456789abcdef".contains($0) })
        #expect(Set(json.keys) == ["credential", "id", "kind", "token"])
        // An alert's info does not open it, so a handle cannot pass for one.
        #expect(throws: NotificationSeal.Failure.open) {
            try NotificationSeal.open(sealed, device: relay.publicKey, agree: relay.sharedSecretFromKeyAgreement)
        }
    }

    @Test func eachHandleHasItsOwnId() throws {
        let credential = try Self.credential(try Self.fixture())
        let one = try credential.handle(token: String(repeating: "ab", count: 32), kind: .alert)
        let two = try credential.handle(token: String(repeating: "ab", count: 32), kind: .alert)
        #expect(one != two)
        #expect(PushRelayCredential.newHandleId() != PushRelayCredential.newHandleId())
    }

    @Test func refusesWhatIsNotAToken() throws {
        let credential = try Self.credential(try Self.fixture())
        for bad in ["", "zz", String(repeating: "a", count: 33), String(repeating: "ab", count: 8), String(repeating: "ab", count: 257)] {
            #expect(throws: PushRelayCredential.Failure.token) { try credential.handle(token: bad, kind: .alert) }
        }
    }

    @Test func refusesAPushRelayKeyThatIsNotItsId() throws {
        let fixture = try Self.fixture()
        let issued = { (id: String, key: String) in
            PushRelayIssued(credential: "c", expiresS: 1, relayKeyId: id, relayKey: key)
        }
        let key = try #require(fixture["relay_key"])
        let keyId = try #require(fixture["relay_key_id"])
        for bad in [issued("00000000", key), issued(keyId, "AAAA"), issued("x", "!")] {
            #expect(throws: PushRelayCredential.Failure.relayKey) {
                try PushRelayCredential(bad, environment: .production)
            }
        }
    }

    @Test func readsThePushRelaysAnswer() throws {
        let json = #"{"credential":"c","expires_s":1800000000,"relay_key_id":"77e8e22a","relay_key":"k"}"#
        let issued = try JSONDecoder().decode(PushRelayIssued.self, from: Data(json.utf8))
        #expect(issued == PushRelayIssued(credential: "c", expiresS: 1_800_000_000, relayKeyId: "77e8e22a", relayKey: "k"))
    }

    @Test func registersAHandleWithItsPushRelay() throws {
        let registration = API.APNsRegistration(
            pushToStart: .pushRelayHandle("h1", environment: .sandbox),
            activities: [.init(pane: "w1-p3", token: .pushRelayHandle("h2", environment: .production))],
            alert: .pushRelayHandle("h3", environment: .production),
            notifyKey: "k"
        )
        let json = try #require(JSONSerialization.jsonObject(with: JSONEncoder().encode(registration)) as? [String: Any])
        #expect(json["push_to_start"] as? [String: String] == ["handle": "h1", "environment": "sandbox"])
        #expect(json["alert"] as? [String: String] == ["handle": "h3", "environment": "production"])
        let activities = json["activities"] as? [[String: Any]]
        #expect(activities?.first?["token"] as? [String: String] == ["handle": "h2", "environment": "production"])
        #expect(json["notify_key"] as? String == "k")
    }

    @Test func readsTheDeliveryAComputerNames() throws {
        let decode = { (json: String) in try JSONDecoder().decode(API.APNsAnswer.self, from: Data(json.utf8)) }
        #expect(try decode(#"{"ok":true,"delivery":"push_relay"}"#).delivery == .pushRelay)
        // An older computer that sent with its own APNs key.
        #expect(try decode(#"{"ok":true,"delivery":"own_key"}"#).delivery == nil)
        #expect(try decode(#"{"ok":true,"delivery":null}"#).delivery == nil)
        #expect(try decode(#"{"ok":true}"#).delivery == nil)
        #expect(try decode(#"{"ok":true,"delivery":"pigeon"}"#).delivery == nil)
    }
}
