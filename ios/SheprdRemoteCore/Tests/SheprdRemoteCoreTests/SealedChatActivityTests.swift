import CryptoKit
import Foundation
import Testing
@testable import SheprdRemoteCore

/// Live Activity content the daemon sealed for the push relay
/// (`Fixtures/push_relay/activity.json`), opened as the widget extension
/// opens it.
@Suite struct SealedChatActivityTests {
    typealias Facts = ActivitySealable<ChatActivityFacts>
    typealias State = ActivitySealable<ChatActivityState>

    static func fixture() throws -> [String: Any] {
        let url = try #require(Bundle.module.url(forResource: "Fixtures/push_relay/activity", withExtension: "json"))
        return try #require(JSONSerialization.jsonObject(with: Data(contentsOf: url)) as? [String: Any])
    }

    static func decode<T: Decodable>(_ type: T.Type, _ value: Any?) throws -> T {
        let value = try #require(value)
        return try JSONDecoder().decode(T.self, from: JSONSerialization.data(withJSONObject: value))
    }

    static func aps(_ fixture: [String: Any], _ push: String) throws -> [String: Any] {
        try #require((fixture[push] as? [String: Any])?["aps"] as? [String: Any])
    }

    static func key(_ fixture: [String: Any], computer: Fingerprint) throws -> SealedChatActivity.Key {
        let text = try #require(fixture["device_private_key"] as? String)
        let raw = try #require(Base64URL.decode(text))
        let device = try P256.KeyAgreement.PrivateKey(rawRepresentation: raw)
        return .init(computer: computer, device: device.publicKey, agree: device.sharedSecretFromKeyAgreement)
    }

    static let owner = Fingerprint(String(repeating: "AB", count: 32))!
    static let other = Fingerprint(String(repeating: "CD", count: 32))!

    @Test func opensTheDaemonsSealedStartAndUpdate() throws {
        let fixture = try Self.fixture()
        let start = try Self.aps(fixture, "start")
        let facts = try Self.decode(Facts.self, start["attributes"])
        let state = try Self.decode(State.self, start["content-state"])
        guard case .sealed = facts, case .sealed = state else {
            Issue.record("the fixture should be sealed")
            return
        }
        let otherKey = try SealedChatActivity.Key(
            computer: Self.other, device: P256.KeyAgreement.PrivateKey().publicKey,
            agree: P256.KeyAgreement.PrivateKey().sharedSecretFromKeyAgreement
        )
        let keys = [otherKey, try Self.key(fixture, computer: Self.owner)]
        let opened = try #require(SealedChatActivity.open(attributes: facts, state: state, keys: keys, allowsPlain: false))
        #expect(opened.facts == (try Self.decode(ChatActivityFacts.self, fixture["attributes"])))
        #expect(opened.state == (try Self.decode(ChatActivityState.self, fixture["content_state"])))
        #expect(opened.state.options.map(\.name) == ["Allow once", "Reject"])
        #expect(opened.state.permission != nil)

        let update = try Self.decode(State.self, try Self.aps(fixture, "update")["content-state"])
        #expect(SealedChatActivity.open(attributes: facts, state: update, keys: keys, allowsPlain: false)?.state == opened.state)
    }

    @Test func sealedContentNamingAnotherComputerShowsNothing() throws {
        let fixture = try Self.fixture()
        let start = try Self.aps(fixture, "start")
        let facts = try Self.decode(Facts.self, start["attributes"])
        let state = try Self.decode(State.self, start["content-state"])
        // The key opens it, but the attributes name a computer other than
        // the one that key belongs to.
        let key = try Self.key(fixture, computer: Self.other)
        #expect(SealedChatActivity.open(attributes: facts, state: state, keys: [key], allowsPlain: true) == nil)
        #expect(SealedChatActivity.facts(facts, keys: [key]) == nil)
        #expect(SealedChatActivity.open(attributes: facts, state: state, keys: [], allowsPlain: true) == nil)
    }

    @Test func plainTextNeverSitsBesideSealedAttributes() throws {
        let fixture = try Self.fixture()
        let facts = try Self.decode(Facts.self, try Self.aps(fixture, "start")["attributes"])
        let plain = State.plain(try Self.decode(ChatActivityState.self, fixture["content_state"]))
        let key = try Self.key(fixture, computer: Self.owner)
        #expect(SealedChatActivity.open(attributes: facts, state: plain, keys: [key], allowsPlain: true) == nil)
    }

    @Test func plainContentShowsOnlyWhenAllowed() throws {
        let fixture = try Self.fixture()
        let facts = try Self.decode(Facts.self, fixture["attributes"])
        let state = try Self.decode(State.self, fixture["content_state"])
        #expect(facts.plain != nil)
        #expect(SealedChatActivity.open(attributes: facts, state: state, keys: [], allowsPlain: true) != nil)
        #expect(SealedChatActivity.open(attributes: facts, state: state, keys: [], allowsPlain: false) == nil)
    }

    @Test func aChangedByteOpensToNothing() throws {
        let fixture = try Self.fixture()
        let start = try Self.aps(fixture, "start")
        let facts = try Self.decode(Facts.self, start["attributes"])
        guard case let .sealed(text) = try Self.decode(State.self, start["content-state"]) else {
            Issue.record("the fixture should be sealed")
            return
        }
        var bytes = try #require(Base64URL.decode(text))
        bytes[bytes.count - 1] ^= 1
        let key = try Self.key(fixture, computer: Self.owner)
        let changed = State.sealed(Base64URL.encode(bytes))
        #expect(SealedChatActivity.open(attributes: facts, state: changed, keys: [key], allowsPlain: false) == nil)
    }

    @Test func keepsTheShapeItCameIn() throws {
        let sealed = try JSONEncoder().encode(State.sealed("abc"))
        #expect(String(decoding: sealed, as: UTF8.self) == #"{"sealed":"abc"}"#)
        let fixture = try Self.fixture()
        let plain = try Self.decode(State.self, fixture["content_state"])
        #expect(try JSONDecoder().decode(State.self, from: JSONEncoder().encode(plain)) == plain)
    }
}
