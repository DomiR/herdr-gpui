import CryptoKit
import Foundation

/// A Live Activity's `attributes` or `content-state` as it arrives: plain,
/// from the app's own debug sample or a computer from before the push
/// relay, or one `sealed` field, from a computer pushing through the push
/// relay
/// (`crates/sheprd-remote-control/src/apns/relayed.rs`), which only this
/// phone's notify key for that computer opens.
public enum ActivitySealable<Value: Codable & Hashable & Sendable>: Codable, Hashable, Sendable {
    case plain(Value)
    case sealed(String)

    private enum Keys: String, CodingKey {
        case sealed
    }

    public init(from decoder: Decoder) throws {
        if let sealed = try decoder.container(keyedBy: Keys.self).decodeIfPresent(String.self, forKey: .sealed) {
            self = .sealed(sealed)
        } else {
            self = .plain(try Value(from: decoder))
        }
    }

    public func encode(to encoder: Encoder) throws {
        switch self {
        case let .plain(value):
            try value.encode(to: encoder)
        case let .sealed(text):
            var container = encoder.container(keyedBy: Keys.self)
            try container.encode(text, forKey: .sealed)
        }
    }

    /// The value when it came plain.
    public var plain: Value? {
        if case let .plain(value) = self { value } else { nil }
    }
}

/// Opens sealed Live Activity content with the phone's notify keys, one
/// per paired computer.
public enum SealedChatActivity {
    /// Names Live Activity content in the seal, so neither an alert nor a
    /// handle passes for it.
    public static let info = Data("sheprd-live-activity-v1".utf8)

    /// One paired computer's notify key. `agree` performs ECDH with the
    /// private half, which may live in the Secure Enclave.
    public struct Key {
        public let computer: Fingerprint
        public let device: P256.KeyAgreement.PublicKey
        public let agree: (P256.KeyAgreement.PublicKey) throws -> SharedSecret

        public init(
            computer: Fingerprint,
            device: P256.KeyAgreement.PublicKey,
            agree: @escaping (P256.KeyAgreement.PublicKey) throws -> SharedSecret
        ) {
            self.computer = computer
            self.device = device
            self.agree = agree
        }
    }

    /// What the activity says, and the computer it is from.
    public struct Opened: Equatable, Sendable {
        public let facts: ChatActivityFacts
        public let state: ChatActivityState
    }

    /// The attributes alone, for routing an activity's token: sealed ones
    /// count only when the computer whose key opens them is the one they
    /// name, as alerts do, so one computer cannot pass its activity off as
    /// another's.
    public static func facts(_ attributes: ActivitySealable<ChatActivityFacts>, keys: [Key]) -> (ChatActivityFacts, Key?)? {
        switch attributes {
        case let .plain(facts):
            return (facts, nil)
        case let .sealed(text):
            for key in keys {
                guard let facts: ChatActivityFacts = open(text, with: key) else { continue }
                return facts.fingerprint == key.computer ? (facts, key) : nil
            }
            return nil
        }
    }

    /// The activity to draw, or `nil` for the fixed cover with no buttons:
    /// nothing from a push that did not open is ever shown.
    ///
    /// - Sealed attributes need sealed state, opened by the same computer's
    ///   key: a push that only some other party could have sent cannot
    ///   change what one computer's activity says.
    /// - Plain attributes come from the debug sample, or a computer from
    ///   before the push relay. They are
    ///   refused when `allowsPlain` is false, as while this phone uses the
    ///   push relay, which then holds its push-to-start token and could
    ///   otherwise start an activity with any text.
    public static func open(
        attributes: ActivitySealable<ChatActivityFacts>,
        state: ActivitySealable<ChatActivityState>,
        keys: [Key],
        allowsPlain: Bool
    ) -> Opened? {
        guard allowsPlain || attributes.plain == nil,
              let (facts, key) = facts(attributes, keys: keys)
        else { return nil }
        switch state {
        case let .plain(state):
            guard case .none = key else { return nil }
            return Opened(facts: facts, state: state)
        case let .sealed(text):
            let owner = key ?? keys.first { $0.computer == facts.fingerprint }
            guard let owner, let state: ChatActivityState = open(text, with: owner) else { return nil }
            return Opened(facts: facts, state: state)
        }
    }

    private static func open<Value: Decodable>(_ text: String, with key: Key) -> Value? {
        guard let sealed = Base64URL.decode(text),
              let plaintext = try? NotificationSeal.open(sealed, device: key.device, info: info, agree: key.agree)
        else { return nil }
        return try? JSONDecoder().decode(Value.self, from: plaintext)
    }
}
