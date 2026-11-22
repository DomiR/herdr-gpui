import CryptoKit
import Foundation
import Testing
@testable import SheprdRemoteCore

/// A test push the phone sends itself through the push relay: what it posts,
/// what the extension opens of it, and what each push relay answer says.
@Suite struct SelfTestPushTests {
    static let id = "6F1D2C3B-0A4E-4F5D-9C8B-7A6E5D4C3B2A"
    static let own = P256.KeyAgreement.PrivateKey()

    static func payload(_ request: SelfTestPush.Request) throws -> [String: Any] {
        try #require(JSONSerialization.jsonObject(with: Data(request.payload.utf8)) as? [String: Any])
    }

    static func sealed(_ request: SelfTestPush.Request) throws -> String {
        try #require(payload(request)["sealed"] as? String)
    }

    @Test func itOpensWithThePhonesOwnKeyToItsIdAndNothingElse() throws {
        let request = try SelfTestPush.request(id: Self.id, handle: "h", device: Self.own.publicKey)
        let opened = try #require(try SelfTestPush.open(Self.sealed(request), device: Self.own.publicKey) {
            try Self.own.sharedSecretFromKeyAgreement(with: $0)
        })
        #expect(opened == SelfTestPush.Opened(id: Self.id, title: SelfTestPush.title, body: SelfTestPush.body))
        // As a plain alert it is a test from no computer, with no actions.
        let alert = try SealedAlert.open(Self.sealed(request), device: Self.own.publicKey) {
            try Self.own.sharedSecretFromKeyAgreement(with: $0)
        }
        #expect(alert.kind == .test && alert.test == Self.id)
        #expect(Fingerprint(alert.computer) == nil)
        #expect(alert.badge == nil && alert.permission == nil && !alert.wizz && !alert.clear)
        #expect(alert.presentation.category == nil && alert.presentation.actions.isEmpty)
        // The extension records the id as it does a computer's test.
        let suite = "SelfTestPushTests-\(UUID().uuidString)"
        let defaults = try #require(UserDefaults(suiteName: suite))
        defer { defaults.removePersistentDomain(forName: suite) }
        PushTestReceipts.record(opened.id, at: Date(timeIntervalSince1970: 5), in: defaults)
        #expect(PushTestReceipts.arrival(of: Self.id, in: defaults) == Date(timeIntervalSince1970: 5))
    }

    @Test func anotherKeyCannotOpenIt() throws {
        let request = try SelfTestPush.request(id: Self.id, handle: "h", device: Self.own.publicKey)
        let computerKey = P256.KeyAgreement.PrivateKey()
        #expect(throws: NotificationSeal.Failure.open) {
            try SelfTestPush.open(Self.sealed(request), device: computerKey.publicKey) {
                try computerKey.sharedSecretFromKeyAgreement(with: $0)
            }
        }
    }

    /// Whatever else opens with the phone's own key is not taken as a test:
    /// a computer's alert, one naming a computer, or a test carrying more.
    @Test func onlyAnExactSelfTestOpensAsOne() throws {
        let fingerprint = String(repeating: "AB", count: 32)
        let base = #"{"v":1,"kind":"test","clear":false,"computer":"this-phone","pane":"test","title":"t","body":"b","test":"x1""#
        let refused = [
            #"{"v":1,"kind":"question","computer":"this-phone","pane":"test","test":"x1"}"#,
            base.replacingOccurrences(of: "this-phone", with: fingerprint) + "}",
            base.replacingOccurrences(of: #""pane":"test""#, with: #""pane":"w1:p2""#) + "}",
            base + #","badge":3}"#,
            base + #","wizz":true}"#,
            base + #","permission":{"id":7,"options":[{"id":"a","name":"Allow","kind":"allow_once"}]}}"#,
            base.replacingOccurrences(of: #""clear":false"#, with: #""clear":true"#) + "}",
            base.replacingOccurrences(of: #""test":"x1""#, with: #""test":"../x""#) + "}",
            #"{"v":1,"kind":"test","computer":"this-phone","pane":"test"}"#,
        ]
        for json in refused {
            let sealed = try NotificationSeal.seal(Data(json.utf8), to: Self.own.publicKey, info: NotificationSeal.info)
            let opened = try SelfTestPush.open(Base64URL.encode(sealed), device: Self.own.publicKey) {
                try Self.own.sharedSecretFromKeyAgreement(with: $0)
            }
            #expect(opened == nil, "\(json)")
        }
        let sealed = try NotificationSeal.seal(Data((base + "}").utf8), to: Self.own.publicKey, info: NotificationSeal.info)
        #expect(try SelfTestPush.open(Base64URL.encode(sealed), device: Self.own.publicKey) {
            try Self.own.sharedSecretFromKeyAgreement(with: $0)
        }?.id == "x1")
    }

    /// The request the push relay takes, and an outer payload shaped as the
    /// daemon's own test push (`Fixtures/notify/test.json`) in everything
    /// but the scoped ids. A computer's thread and collapse id carry the
    /// pairing's scope (`apns::alert::scope`), taken from the notify key
    /// the phone gave that computer; a self test has no computer, so it
    /// keeps the fixed thread `test` and the collapse id `test-ID`.
    @Test func theRequestMatchesThePushRelayAndTheDaemonsTestPush() throws {
        let request = try SelfTestPush.request(id: Self.id, handle: "k.h", device: Self.own.publicKey)
        let body = try #require(JSONSerialization.jsonObject(with: JSONEncoder().encode(request)) as? [String: String])
        #expect(Set(body.keys) == ["collapse_id", "handle", "payload", "priority", "push_type"])
        #expect(body["push_type"] == "alert" && body["priority"] == "high" && body["handle"] == "k.h")
        #expect(body["collapse_id"] == "test-\(Self.id)")
        #expect(request.payload.utf8.count <= 4096)

        let url = try #require(Bundle.module.url(forResource: "Fixtures/notify/test", withExtension: "json"))
        let fixture = try #require(JSONSerialization.jsonObject(with: Data(contentsOf: url)) as? [String: Any])
        let daemon = try #require(fixture["apns"] as? [String: Any])
        let ours = try Self.payload(request)
        #expect(Set(ours.keys) == Set(daemon.keys))
        var ourAps = try #require(ours["aps"] as? [String: Any])
        var daemonAps = try #require(daemon["aps"] as? [String: Any])
        #expect(ourAps["thread-id"] as? String == SelfTestPush.pane)
        let daemonThread = try #require(daemonAps["thread-id"] as? String)
        #expect(daemonThread.hasSuffix("/\(SelfTestPush.pane)") && daemonThread != SelfTestPush.pane)
        ourAps["thread-id"] = nil
        daemonAps["thread-id"] = nil
        #expect(NSDictionary(dictionary: ourAps) == NSDictionary(dictionary: daemonAps))
        // The sealed side has the daemon's keys, less the badge.
        let plaintext = try #require(fixture["plaintext"] as? [String: Any])
        let opened = try NotificationSeal.open(
            #require(Base64URL.decode(Self.sealed(request))), device: Self.own.publicKey
        ) { try Self.own.sharedSecretFromKeyAgreement(with: $0) }
        let mine = try #require(JSONSerialization.jsonObject(with: opened) as? [String: Any])
        #expect(Set(mine.keys) == Set(plaintext.keys).subtracting(["badge"]))
    }

    /// The longest id still makes a collapse id the push relay takes.
    @Test func theLongestIdIsCutToTheCollapseIdLimit() throws {
        let id = String(repeating: "a", count: SelfTestPush.maxId)
        let request = try SelfTestPush.request(id: id, handle: "h", device: Self.own.publicKey)
        #expect(request.collapseId.utf8.count == SelfTestPush.maxCollapseId)
        #expect(request.collapseId.hasPrefix("test-aaaa"))
        // The full id still travels sealed, which the receipt is keyed by.
        let opened = try SelfTestPush.open(Self.sealed(request), device: Self.own.publicKey) {
            try Self.own.sharedSecretFromKeyAgreement(with: $0)
        }
        #expect(opened?.id == id)
    }

    @Test func aBadIdIsNotSent() {
        for id in ["", String(repeating: "a", count: 65), "a b", "é"] {
            #expect(throws: SelfTestPush.Failure.id) {
                try SelfTestPush.request(id: id, handle: "h", device: Self.own.publicKey)
            }
        }
    }
}

@Suite struct PushRelayOutcomeTests {
    @Test func outcomesAreReadByNameThenStatus() {
        let named: [(String, PushRelayOutcome)] = [
            ("delivered", .delivered), ("expired", .expired), ("gone", .gone), ("limited", .limited),
            ("refused", .refused), ("invalid", .invalid), ("unavailable", .unavailable),
        ]
        for (name, outcome) in named {
            #expect(PushRelayOutcome(status: 200, outcome: name) == outcome)
        }
        #expect(PushRelayOutcome(status: 401, outcome: nil) == .expired)
        #expect(PushRelayOutcome(status: 410, outcome: nil) == .gone)
        #expect(PushRelayOutcome(status: 429, outcome: "new") == .limited)
        #expect(PushRelayOutcome(status: 403, outcome: nil) == .refused)
        #expect(PushRelayOutcome(status: 503, outcome: nil) == .unavailable)
        #expect(PushRelayOutcome(status: 418, outcome: nil) == .unknown(status: 418))
    }

    @Test func eachFailureSaysWhereItStopped() {
        #expect(PushRelayOutcome.delivered.failure == nil)
        #expect(PushRelayOutcome.expired.failure?.contains("attests again") == true)
        #expect(PushRelayOutcome.gone.failure?.contains("token") == true)
        #expect(PushRelayOutcome.limited.failure?.contains("limiting") == true)
        #expect(PushRelayOutcome.refused.failure?.contains("refused") == true)
        #expect(PushRelayOutcome.unavailable.failure?.contains("unavailable") == true)
        #expect(PushRelayOutcome.unknown(status: 418).failure?.contains("418") == true)
        #expect(PushRelayOutcome.expired.needsNewCredential && PushRelayOutcome.refused.needsNewCredential)
        #expect(!PushRelayOutcome.gone.needsNewCredential && !PushRelayOutcome.limited.needsNewCredential)
    }
}
