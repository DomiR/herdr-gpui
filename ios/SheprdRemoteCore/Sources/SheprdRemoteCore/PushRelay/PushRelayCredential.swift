import CryptoKit
import Foundation

/// Which push relay, and so which APNs environment, this build's tokens
/// belong to. Xcode builds get sandbox tokens and attest in App Attest's
/// development environment; TestFlight and App Store builds get production
/// tokens and attest in production. Each push relay refuses the other's.
public enum PushRelayEnvironment: String, Codable, Hashable, Sendable {
    case production
    case sandbox

    /// The push relay's host. Compiled in, as on the computer, so nothing
    /// a computer or a push says can point the phone elsewhere.
    public var host: String {
        switch self {
        case .production: "push.sheprd.org"
        case .sandbox: "push-sandbox.sheprd.org"
        }
    }

    /// `path` on this push relay, over HTTPS.
    public func url(_ path: String) -> URL? {
        URL(string: "https://\(host)\(path)")
    }

    /// The environment of a build with this embedded provisioning profile.
    /// A development profile says `aps-environment` is `development`. App
    /// Store and TestFlight builds carry no profile, and an ad hoc one says
    /// `production`.
    public init(provisioningProfile: Data?) {
        self = Self.apsEnvironment(in: provisioningProfile) == "development" ? .sandbox : .production
    }

    /// `aps-environment` in a profile's entitlements. The profile is a CMS
    /// envelope around an XML property list, which is read as it stands:
    /// the signature is the system's to check, not the app's.
    static func apsEnvironment(in profile: Data?) -> String? {
        guard let profile,
              let start = profile.range(of: Data("<?xml".utf8)),
              let end = profile.range(of: Data("</plist>".utf8), in: start.lowerBound ..< profile.endIndex)
        else { return nil }
        let plist = try? PropertyListSerialization.propertyList(
            from: profile.subdata(in: start.lowerBound ..< end.upperBound), format: nil
        ) as? [String: Any]
        return (plist?["Entitlements"] as? [String: Any])?["aps-environment"] as? String
    }
}

/// What `POST /v1/attest` answers an attested phone.
public struct PushRelayIssued: Decodable, Equatable, Sendable {
    public let credential: String
    public let expiresS: UInt64
    public let relayKeyId: String
    /// The push relay's public key, an uncompressed SEC1 point in base64url.
    public let relayKey: String

    enum CodingKeys: String, CodingKey {
        case credential
        case expiresS = "expires_s"
        case relayKeyId = "relay_key_id"
        case relayKey = "relay_key"
    }

    public init(credential: String, expiresS: UInt64, relayKeyId: String, relayKey: String) {
        self.credential = credential
        self.expiresS = expiresS
        self.relayKeyId = relayKeyId
        self.relayKey = relayKey
    }
}

/// The push relay's word that this phone passed App Attest, kept in the
/// main app's keychain, and the key its handles are sealed to. It lasts 30
/// days; renewal by assertion is not built, so the app attests a new key
/// once it nears its end.
public struct PushRelayCredential: Codable, Equatable, Sendable {
    public enum Failure: Error, Equatable {
        /// The push relay's key is not a P-256 point, or its id is not the
        /// one the push relay derives from it.
        case relayKey
        /// Not a push token as hex.
        case token
    }

    /// How long before it runs out a credential is replaced, so a phone
    /// that opens the app every few weeks never sends a lapsed one.
    public static let renewal: TimeInterval = 7 * 24 * 60 * 60

    public let credential: String
    public let environment: PushRelayEnvironment
    public let expires: Date
    public let relayKeyId: String
    public let relayKey: String

    /// Checks the push relay's answer before keeping it.
    public init(_ issued: PushRelayIssued, environment: PushRelayEnvironment) throws {
        guard let point = Base64URL.decode(issued.relayKey),
              (try? P256.KeyAgreement.PublicKey(x963Representation: point)) != nil,
              Self.keyId(of: point) == issued.relayKeyId
        else { throw Failure.relayKey }
        credential = issued.credential
        self.environment = environment
        expires = Date(timeIntervalSince1970: TimeInterval(issued.expiresS))
        relayKeyId = issued.relayKeyId
        relayKey = issued.relayKey
    }

    /// The id the push relay names its key by: the first four bytes of the
    /// SHA-256 of its SEC1 point, as lowercase hex.
    static func keyId(of point: Data) -> String {
        SHA256.hash(data: point).prefix(4).map { String(format: "%02x", $0) }.joined()
    }

    /// Whether handles made with it still work at `now`.
    public func isValid(at now: Date) -> Bool {
        now < expires
    }

    /// Whether to attest again at `now`: it has run out or soon will.
    public func needsRenewal(at now: Date) -> Bool {
        now >= expires.addingTimeInterval(-Self.renewal)
    }

    /// A handle for `token`: the token and this credential sealed to the
    /// push relay, with a random id the handle alone is revoked by.
    /// `ephemeral`, `nonce`, and `id` are fixed only in tests.
    public func handle(
        token: String,
        kind: PushRelayKind,
        id: String = PushRelayCredential.newHandleId(),
        ephemeral: P256.KeyAgreement.PrivateKey = P256.KeyAgreement.PrivateKey(),
        nonce: AES.GCM.Nonce = AES.GCM.Nonce()
    ) throws -> String {
        let token = token.lowercased()
        guard token.count.isMultiple(of: 2), (32 ... 512).contains(token.count),
              token.allSatisfy({ $0.isHexDigit && $0.isASCII })
        else { throw Failure.token }
        guard let point = Base64URL.decode(relayKey),
              let key = try? P256.KeyAgreement.PublicKey(x963Representation: point)
        else { throw Failure.relayKey }
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.sortedKeys, .withoutEscapingSlashes]
        let plaintext = try encoder.encode(Opened(credential: credential, id: id, kind: kind, token: token))
        let sealed = try NotificationSeal.seal(
            plaintext, to: key, info: PushRelayKind.handleInfo, ephemeral: ephemeral, nonce: nonce
        )
        return "\(relayKeyId).\(Base64URL.encode(sealed))"
    }

    /// 128 random bits as lowercase hex, the form the push relay takes.
    public static func newHandleId() -> String {
        SymmetricKey(size: .bits128).withUnsafeBytes { bytes in
            bytes.map { String(format: "%02x", $0) }.joined()
        }
    }

    /// What a handle holds, as `crates/sheprd-push-relay/src/handle.rs`
    /// reads it.
    struct Opened: Encodable {
        let credential: String
        let id: String
        let kind: PushRelayKind
        let token: String
    }
}

/// Which token a handle holds, which decides what the push relay lets it
/// carry: alerts, or Live Activity pushes.
public enum PushRelayKind: String, Codable, Sendable {
    case activity
    case alert
    case pushToStart = "push_to_start"

    /// Names a handle in the seal, so it cannot pass for an alert.
    public static let handleInfo = Data("sheprd-relay-handle-v1".utf8)
}
