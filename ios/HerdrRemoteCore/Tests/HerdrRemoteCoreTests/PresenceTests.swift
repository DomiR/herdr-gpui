import CryptoKit
import Foundation
import Testing
@testable import HerdrRemoteCore

@Suite struct PresenceTests {
    @Test func theSignedMessageIsThePrefixAndNonce() throws {
        let nonce = Data(repeating: 7, count: 32)
        #expect(try Presence.message(nonce: Base64URL.encode(nonce)) == Data("herdr-presence-v1".utf8) + nonce)
        #expect(throws: Presence.Failure.nonce) { try Presence.message(nonce: Base64URL.encode(Data(count: 31))) }
    }

    @Test func framesUseTheContractsKeys() throws {
        let hello = try JSONSerialization.jsonObject(with: JSONEncoder().encode(Presence.Outgoing.hello(device: "d1"))) as? [String: String]
        #expect(hello == ["type": "hello", "device": "d1"])
        let near = try JSONSerialization.jsonObject(with: JSONEncoder().encode(Presence.Outgoing.near(true))) as? [String: AnyHashable]
        #expect(near == ["type": "near", "near": true])
        let decode = { (json: String) in try JSONDecoder().decode(Presence.Incoming.self, from: Data(json.utf8)) }
        #expect(try decode(#"{"type":"challenge","nonce":"abc"}"#) == .challenge(nonce: "abc"))
        #expect(try decode(#"{"type":"proven","addresses":null}"#) == .proven(addresses: nil))
        #expect(try decode(#"{"type":"error","message":"unknown device"}"#) == .error("unknown device"))
        #expect(try decode(#"{"type":"later"}"#) == .other)
    }

    @Test func nearnessHasAGapBetweenItsLevels() {
        var nearness = Presence.Nearness()
        #expect(nearness.update(rssi: -70) == nil)
        #expect(nearness.update(rssi: -60) == true)
        #expect(nearness.update(rssi: -75) == nil)
        #expect(nearness.update(rssi: 127) == nil)
        #expect(nearness.update(rssi: -85) == false)
        #expect(nearness.update(rssi: -70) == nil)
    }

    @Test func receivedAddressesComeFirstAndOnlyHTTPSOrigins() {
        let merged = Presence.merged(
            known: ["https://10.0.0.2:7443", "https://mac.local:7443"],
            received: ["https://192.168.1.9:7443", "http://evil:80", "https://10.0.0.2:7443", "https://x:1/path"]
        )
        #expect(merged == ["https://192.168.1.9:7443", "https://10.0.0.2:7443", "https://mac.local:7443"])
    }

    @Test func oldComputerRecordsDecodeWithPresenceOff() throws {
        let fingerprint = String(repeating: "AB", count: 32)
        let json = """
        {"device":"d","name":"Mac","origins":["https://a:1"],"fingerprint":"\(fingerprint)","keyKind":"software","keyData":""}
        """
        let computer = try JSONDecoder().decode(Computer.self, from: Data(json.utf8))
        #expect(!computer.presence)
    }
}

/// The daemon's own presence fixture (`Fixtures/presence/prove.json`,
/// written by `auth/tests/presence.rs`): the phone signs the same bytes,
/// in the encoding the daemon verifies, and opens the sealed addresses.
@Suite struct PresenceFixtureTests {
    struct Fixture: Decodable {
        let device_private_key: String
        let device_public_key: String
        let nonce: String
        let message: String
        let signature: String
        let addresses: Addresses
        struct Addresses: Decodable {
            let device_notify_private_key: String
            let sealed: String
            let plaintext: Plaintext
        }
        struct Plaintext: Decodable { let origins: [String] }
    }

    static func fixture() throws -> Fixture {
        let url = try #require(Bundle.module.url(forResource: "Fixtures/presence/prove", withExtension: "json"))
        return try JSONDecoder().decode(Fixture.self, from: Data(contentsOf: url))
    }

    @Test func thePhoneSignsTheBytesTheDaemonChecks() throws {
        let fixture = try Self.fixture()
        let message = try Presence.message(nonce: fixture.nonce)
        #expect(Base64URL.encode(message) == fixture.message)
        let key = try P256.Signing.PrivateKey(rawRepresentation: #require(Base64URL.decode(fixture.device_private_key)))
        #expect(Base64URL.encode(key.publicKey.x963Representation) == fixture.device_public_key)
        // The daemon's signature verifies here, and the phone's own does
        // in the same r||s encoding (ECDSA signatures are randomized).
        let theirs = try P256.Signing.ECDSASignature(rawRepresentation: #require(Base64URL.decode(fixture.signature)))
        #expect(key.publicKey.isValidSignature(theirs, for: message))
        let ours = try key.signature(for: message)
        #expect(ours.rawRepresentation.count == 64)
        #expect(key.publicKey.isValidSignature(ours, for: message))
    }

    @Test func theSealedAddressesOpenOnlyAsAddresses() throws {
        let fixture = try Self.fixture()
        let key = try P256.KeyAgreement.PrivateKey(rawRepresentation: #require(Base64URL.decode(fixture.addresses.device_notify_private_key)))
        let origins = try Presence.openAddresses(fixture.addresses.sealed, device: key.publicKey) { try key.sharedSecretFromKeyAgreement(with: $0) }
        #expect(origins == fixture.addresses.plaintext.origins)
        // The same bytes do not open as an alert: the info differs.
        #expect(throws: (any Error).self) {
            try SealedAlert.open(fixture.addresses.sealed, device: key.publicKey) { try key.sharedSecretFromKeyAgreement(with: $0) }
        }
    }
}
