import CryptoKit
import Foundation

/// A test push this phone sends itself through the push relay, so the push
/// relay, APNs, and the Notification Service Extension can be checked with
/// no computer online. The phone seals it as a computer seals its test
/// alert (`crates/sheprd-remote-control/src/apns/alert.rs`, `Sealed::test`),
/// under `NotificationSeal.info`, in the daemon's outer payload, but to a
/// key of its own that no computer is given (`NotifyKey`'s own key), and
/// names `sender` as the computer: not a fingerprint, so it can never pass
/// for a computer's alert, and the extension opens it only as a test, which
/// carries no chat, badge, permission, or actions.
public enum SelfTestPush {
    /// What a self test names in place of a computer's fingerprint.
    public static let sender = "this-phone"
    /// The thread a self test goes in (`TEST_PANE`), which names no chat.
    /// A computer's test push puts its pairing scope in front
    /// (`scope/test`); a self test has no computer, so no scope.
    public static let pane = "test"
    public static let title = "Test push"
    public static let body = "From this iPhone: the push relay reaches this phone."
    /// The longest test id, as the daemon bounds it (`MAX_TEST_ID`).
    static let maxId = 64
    /// The longest `apns-collapse-id` APNs and the push relay take, in bytes.
    static let maxCollapseId = 64

    /// `POST /v1/push` on the push relay (`crates/sheprd-push-relay/src/relay.rs`).
    public struct Request: Encodable, Equatable, Sendable {
        public let collapseId: String
        public let handle: String
        /// The APNs payload as JSON text.
        public let payload: String
        public let priority: String
        public let pushType: String

        enum CodingKeys: String, CodingKey {
            case collapseId = "collapse_id"
            case handle, payload, priority
            case pushType = "push_type"
        }
    }

    /// The request that pushes test `id` through `handle` (one made for
    /// this phone's alert token), sealed to `device`. `ephemeral` and
    /// `nonce` are fixed only in tests.
    public static func request(
        id: String,
        handle: String,
        device: P256.KeyAgreement.PublicKey,
        ephemeral: P256.KeyAgreement.PrivateKey = P256.KeyAgreement.PrivateKey(),
        nonce: AES.GCM.Nonce = AES.GCM.Nonce()
    ) throws -> Request {
        guard validId(id) else { throw Failure.id }
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.sortedKeys, .withoutEscapingSlashes]
        let plaintext = try encoder.encode(Plaintext(test: id, event: id))
        let sealed = try NotificationSeal.seal(plaintext, to: device, info: NotificationSeal.info, ephemeral: ephemeral, nonce: nonce)
        let outer = Outer(aps: .init(), sealed: Base64URL.encode(sealed))
        let payload = try encoder.encode(outer)
        return Request(
            // Cut as the daemon cuts its tag; an id is ASCII, so a
            // character is a byte.
            collapseId: String("test-\(id)".prefix(maxCollapseId)),
            handle: handle,
            payload: String(decoding: payload, as: UTF8.self),
            // A test is waited for, so it may not wait either, as the daemon's.
            priority: "high",
            pushType: "alert"
        )
    }

    public enum Failure: Error, Equatable {
        /// Not 1 to 64 letters, digits, or dashes, as a UUID is.
        case id
    }

    /// Whether `id` can be a test push's id, as the daemon's `valid_test_id`.
    public static func validId(_ id: String) -> Bool {
        (1 ... maxId).contains(id.utf8.count) && id.unicodeScalars.allSatisfy { $0.isASCII && ($0 == "-" || $0.properties.isAlphabetic || ("0" ... "9").contains($0)) }
    }

    /// An opened self test: only its id and text, so nothing else in it can
    /// reach a notification.
    public struct Opened: Equatable, Sendable {
        public let id: String
        public let title: String
        public let body: String
    }

    /// Opens a payload's `sealed` field with this phone's own key. `nil`
    /// when it opens but is not a self test exactly as this phone seals one:
    /// a test kind with an id, the sender, the test thread, and no badge,
    /// permission, wizz, or clear.
    public static func open(
        _ sealed: String,
        device: P256.KeyAgreement.PublicKey,
        agree: (P256.KeyAgreement.PublicKey) throws -> SharedSecret
    ) throws -> Opened? {
        let alert = try SealedAlert.open(sealed, device: device, agree: agree)
        guard alert.kind == .test, alert.computer == sender, alert.pane == pane,
              let id = alert.test, validId(id),
              alert.badge == nil, alert.permission == nil, !alert.wizz, !alert.clear
        else { return nil }
        return Opened(id: id, title: alert.title, body: alert.body)
    }

    /// The sealed side: `SealedAlert`'s keys, as `Sealed::test` writes them,
    /// without a badge, since no computer's count is in it.
    struct Plaintext: Encodable {
        let v = 1
        let kind = "test"
        let clear = false
        let computer = SelfTestPush.sender
        let pane = SelfTestPush.pane
        let title = SelfTestPush.title
        let body = SelfTestPush.body
        let test: String
        /// Its own decision, as every alert's: the test's id serves.
        let event: String
    }

    /// The daemon's outer payload for a test alert: the cover, mutable
    /// content for the extension, and the test thread.
    struct Outer: Encodable {
        let aps: Aps
        let sealed: String
    }

    struct Aps: Encodable {
        let alert = Cover()
        let interruptionLevel = "active"
        let mutableContent = 1
        let sound = "default"
        let threadId = SelfTestPush.pane

        enum CodingKeys: String, CodingKey {
            case alert
            case interruptionLevel = "interruption-level"
            case mutableContent = "mutable-content"
            case sound
            case threadId = "thread-id"
        }
    }

    struct Cover: Encodable {
        let title = "Sheprd"
        let body = "New activity"
    }
}

/// What the push relay answered a push (`{"outcome": …}`), and what each
/// means for a phone testing itself.
public enum PushRelayOutcome: Equatable, Sendable {
    case delivered
    /// The credential ran out: the phone must attest again.
    case expired
    /// APNs no longer knows the token, or the handle was revoked.
    case gone
    case limited
    /// The push relay would not open the handle, or its kind does not
    /// allow alerts: a handle from an old key or credential.
    case refused
    /// The push relay found the request malformed.
    case invalid
    /// The push relay, its store, or APNs is down for now.
    case unavailable
    /// An answer this app does not know, with its HTTP status.
    case unknown(status: Int)

    /// Reads the push relay's answer: its `outcome` field when there is
    /// one, the HTTP status otherwise.
    public init(status: Int, outcome: String?) {
        switch outcome {
        case "delivered": self = .delivered
        case "expired": self = .expired
        case "gone": self = .gone
        case "limited": self = .limited
        case "refused": self = .refused
        case "invalid": self = .invalid
        case "unavailable": self = .unavailable
        default:
            switch status {
            case 401: self = .expired
            case 410: self = .gone
            case 429: self = .limited
            case 403: self = .refused
            case 503: self = .unavailable
            default: self = .unknown(status: status)
            }
        }
    }

    /// Whether this phone should attest again before its next push: its
    /// credential, or the handles made with it, no longer work.
    public var needsNewCredential: Bool {
        self == .expired || self == .refused
    }

    /// What Settings says when a self test stopped here; `nil` once it was
    /// delivered to APNs.
    public var failure: String? {
        switch self {
        case .delivered:
            nil
        case .expired:
            "The push relay credential ran out. This phone attests again on the next try; send it again."
        case .gone:
            "APNs no longer knows this phone's notification token. Reopen the app to get a new one, then try again."
        case .limited:
            "The push relay or APNs is limiting pushes to this phone. Try again in a minute."
        case .refused:
            "The push relay refused this phone's handle. This phone attests again on the next try; send it again."
        case .invalid:
            "The push relay could not read the test push. Update the app."
        case .unavailable:
            "The push relay or APNs is unavailable right now. Try again later."
        case let .unknown(status):
            "The push relay answered HTTP \(status)."
        }
    }
}
