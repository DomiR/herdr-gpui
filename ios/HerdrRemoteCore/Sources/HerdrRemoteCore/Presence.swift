import CryptoKit
import Foundation

/// Bluetooth presence after pairing (`docs/ios-app-plan.md`, "Presence,
/// exactly"): a phone near a paired computer proves itself on the
/// `presence` characteristic by signing the computer's challenge with its
/// device key, then says whether it is near, and the computer holds phone
/// alerts while you are at it. The computer answers a proof with its
/// current addresses, sealed to the phone, so a computer whose address
/// changed is found again without Bonjour.
public enum Presence {
    /// What the device key signs: this, then the 32-byte nonce.
    public static let prefix = Data("herdr-presence-v1".utf8)
    /// Seals the addresses, apart from alerts (`NotificationSeal.info`).
    public static let addressesInfo = Data("herdr-remote-addresses-v1".utf8)
    /// The most addresses kept for a computer.
    public static let maxOrigins = 8

    public enum Failure: Error, Equatable {
        case nonce
    }

    /// The bytes to sign for a challenge's base64url `nonce`.
    public static func message(nonce: String) throws -> Data {
        guard let decoded = Base64URL.decode(nonce), decoded.count == 32 else { throw Failure.nonce }
        return prefix + decoded
    }

    /// Frames the phone writes.
    public enum Outgoing: Encodable, Hashable, Sendable {
        case hello(device: String)
        case prove(device: String, signature: String)
        case near(Bool)

        enum CodingKeys: String, CodingKey { case type, device, signature, near }

        public func encode(to encoder: Encoder) throws {
            var c = encoder.container(keyedBy: CodingKeys.self)
            switch self {
            case let .hello(device):
                try c.encode("hello", forKey: .type)
                try c.encode(device, forKey: .device)
            case let .prove(device, signature):
                try c.encode("prove", forKey: .type)
                try c.encode(device, forKey: .device)
                try c.encode(signature, forKey: .signature)
            case let .near(near):
                try c.encode("near", forKey: .type)
                try c.encode(near, forKey: .near)
            }
        }
    }

    /// Frames the computer sends.
    public enum Incoming: Decodable, Hashable, Sendable {
        case challenge(nonce: String)
        /// Sealed addresses, or nil when the phone registered no key.
        case proven(addresses: String?)
        case error(String)
        case other

        enum CodingKeys: String, CodingKey { case type, nonce, addresses, message }

        public init(from decoder: Decoder) throws {
            let c = try decoder.container(keyedBy: CodingKeys.self)
            switch try c.decode(String.self, forKey: .type) {
            case "challenge": self = .challenge(nonce: try c.decode(String.self, forKey: .nonce))
            case "proven": self = .proven(addresses: try c.decodeIfPresent(String.self, forKey: .addresses))
            case "error": self = .error(try c.decodeIfPresent(String.self, forKey: .message) ?? "")
            default: self = .other
            }
        }
    }

    struct Addresses: Decodable {
        let origins: [String]
    }

    /// The computer's current addresses from a `proven` frame.
    public static func openAddresses(
        _ sealed: String,
        device: P256.KeyAgreement.PublicKey,
        agree: (P256.KeyAgreement.PublicKey) throws -> SharedSecret
    ) throws -> [String] {
        guard let bytes = Base64URL.decode(sealed) else { throw NotificationSeal.Failure.format }
        let plaintext = try NotificationSeal.open(bytes, device: device, info: addressesInfo, agree: agree)
        return try JSONDecoder().decode(Addresses.self, from: plaintext).origins
    }

    /// The addresses to keep: the ones the computer just sent first, as
    /// they are current, then the known ones, only `https` origins, once
    /// each, at most `maxOrigins`.
    public static func merged(known: [String], received: [String]) -> [String] {
        var seen = Set<String>()
        return (received + known)
            .filter { origin in
                guard let url = URL(string: origin), url.scheme == "https", url.host != nil, url.path.isEmpty || url.path == "/" else { return false }
                return seen.insert(origin).inserted
            }
            .prefix(maxOrigins)
            .map { $0 }
    }

    /// Near or not from signal strength, with a gap between the levels so a
    /// phone at the edge does not flap.
    public struct Nearness: Hashable, Sendable {
        public static let enter = -65
        public static let leave = -80
        public private(set) var near = false

        public init() {}

        /// Takes a reading; answers the new state when it changed.
        public mutating func update(rssi: Int) -> Bool? {
            // 127 means the radio could not read it.
            guard rssi != 127 else { return nil }
            if !near, rssi >= Self.enter {
                near = true
                return true
            }
            if near, rssi <= Self.leave {
                near = false
                return false
            }
            return nil
        }
    }
}
