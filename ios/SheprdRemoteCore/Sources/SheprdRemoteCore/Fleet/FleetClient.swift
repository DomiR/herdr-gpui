import CryptoKit
import Foundation

extension Fleet {
    /// This phone's fleet key, as the app holds it: its public half and a
    /// way to sign. On a phone the key is in the Secure Enclave with user
    /// presence, so `sign` asks for Face ID or the passcode.
    public struct Signer: Sendable {
        /// SEC1 compressed.
        public let publicKey: Data
        /// ECDSA P-256 with SHA-256, `r || s`.
        public let sign: @Sendable (Data) throws -> Data

        public init(publicKey: Data, sign: @escaping @Sendable (Data) throws -> Data) {
            self.publicKey = publicKey
            self.sign = sign
        }

        /// The member id: the SHA-256 of the key.
        public var member: Data { Data(SHA256.hash(data: publicKey)) }
    }

    /// A phone's bundle: its fleet key and relay keys, and nothing else, as
    /// a phone gets the API only, never SSH.
    public static func phoneBundle(key: Data, name: String, relay: RelayKeyPair) throws -> Bundle {
        guard let dial = try? P256.Signing.PrivateKey(rawRepresentation: relay.dialKey),
              let tunnel = try? Curve25519.KeyAgreement.PrivateKey(rawRepresentation: relay.tunnelKey)
        else { throw FleetError.invalid }
        return Bundle(
            key: key, kind: .phone, name: Self.memberName(name),
            dial: dial.publicKey.compressedRepresentation, tunnel: tunnel.publicKey.rawRepresentation,
            ssh: nil, host: nil
        )
    }

    /// A name the daemon takes: no control characters, at most 64 bytes.
    static func memberName(_ name: String) -> String {
        var name = String(name.unicodeScalars.filter { !CharacterSet.controlCharacters.contains($0) })
        while name.utf8.count > maxName { name.removeLast() }
        return name.trimmingCharacters(in: .whitespaces).isEmpty ? "iPhone" : name
    }

    /// An operation for `signer` to sign on what a computer answered.
    public static func draft(on next: API.FleetNext, signer: Data, issuedMs: UInt64, body: Body) throws -> Op {
        guard let fleet = Data(hex: next.fleet) else { throw FleetError.encoding }
        let parents = try next.parents.map { text -> Data in
            guard let parent = Data(hex: text) else { throw FleetError.encoding }
            return parent
        }
        return Op(
            fleet: fleet, signer: signer, seq: next.seq, issuedMs: issuedMs,
            parents: parents.sorted { $0.lexicographicallyPrecedes($1) }, body: body, signature: Data()
        )
    }

    /// An id (member, host, fleet, or operation) from hex; nil for
    /// anything but 32 bytes.
    public static func data(hex: String) -> Data? {
        Data(hex: hex).flatMap { $0.count == 32 ? $0 : nil }
    }

    /// The device id a host gives the key a `Link` names: `fleet-` and the
    /// first 16 bytes of the SHA-256 of the key, uncompressed, in hex
    /// (`fleet::links::device_id`).
    public static func linkDevice(key: Data) -> String? {
        let point: P256.Signing.PublicKey? = switch key.count {
        case 65: try? P256.Signing.PublicKey(x963Representation: key)
        case 33: try? P256.Signing.PublicKey(compressedRepresentation: key)
        default: nil
        }
        guard let point else { return nil }
        return "fleet-" + Data(SHA256.hash(data: point.x963Representation).prefix(16)).hexString
    }
}

extension Fleet.Op {
    /// Signs it with `sign`, which must be the signer's key.
    public mutating func sign(using sign: (Data) throws -> Data) throws {
        signature = try sign(signedBytes())
        guard signature.count == 64 else { throw FleetError.signature }
    }
}

extension Data {
    /// Reads hex in either case; nil for anything else.
    init?(hex: String) {
        guard hex.count.isMultiple(of: 2) else { return nil }
        var data = Data(capacity: hex.count / 2)
        var index = hex.startIndex
        while index < hex.endIndex {
            let next = hex.index(index, offsetBy: 2)
            guard let byte = UInt8(hex[index..<next], radix: 16) else { return nil }
            data.append(byte)
            index = next
        }
        self = data
    }
}

/// Why a fleet call did not do what it was asked.
public enum FleetClientError: Error, Equatable, Sendable {
    /// The computer is in no fleet.
    case noFleet
    /// This phone is not a member there, or not yet.
    case notMember
    /// The computer refused operations this phone signed.
    case refused(Int)
}

/// The fleet calls of one computer of the fleet, over a transport the app
/// supplies (a pinned session with a bearer token). The phone signs on
/// its own copy of the log (`SheprdFleetLog`), never on what a computer
/// answers; the computer checks what it posts as it checks any member's.
public struct FleetClient: Sendable {
    /// Sends `body` (JSON) with `method` to `path`; answers the response
    /// body, throwing for any status but success.
    public typealias Send = @Sendable (_ method: String, _ path: String, _ body: Data?) async throws -> Data

    private let send: Send

    public init(send: @escaping Send) {
        self.send = send
    }

    /// Calls through the computer's live connection when it has one, asked
    /// afresh for every call (`live`), relay included; through `fallback`
    /// (signing in at its saved addresses) only while it has none. A
    /// saved address may be the network the phone left; the live link is
    /// the path that answers now.
    public static func preferring(live: @escaping @Sendable () async -> Send?, fallback: @escaping Send) -> FleetClient {
        FleetClient { method, path, body in
            let send = await live() ?? fallback
            return try await send(method, path, body)
        }
    }

    /// The fleet, and what `member` signs on next when it is one.
    public func status(member: Data?) async throws -> API.FleetStatus {
        let path = member.map { "/api/v2/fleet?member=\($0.hexString)" } ?? "/api/v2/fleet"
        var status = try JSONDecoder().decode(API.FleetStatus.self, from: await send("GET", path, nil))
        status.member = member?.hexString
        return status
    }

    /// Posts `ops` (base64url) for `fleet`, which a computer of another
    /// fleet refuses; answers the computer's heads and what the phone
    /// lacks after `heads`.
    public func sync(_ ops: [String], heads: [String], fleet: String) async throws -> API.FleetApplied {
        guard ops.count <= Self.maxSyncOps, heads.count <= Self.maxSyncHeads else { throw FleetError.bounds }
        let body = API.FleetSync(fleet: fleet, heads: heads, ops: ops)
        let data = try await send("POST", "/api/v2/fleet/sync", try JSONEncoder().encode(body))
        return try JSONDecoder().decode(API.FleetApplied.self, from: data)
    }

    /// Operations one sync may carry: the daemon refuses more.
    public static let maxSyncOps = 64

    /// Heads one sync may name: the daemon refuses more. They only say
    /// what the phone knows, so fewer only make the answer longer.
    public static let maxSyncHeads = 64

    /// Asks to join the computer's fleet with `bundle`, signed with
    /// `signer` for `fleet`; a person admits it in Sheprd.
    public func join(_ bundle: Fleet.Bundle, fleet: String, as signer: Fleet.Signer) async throws -> API.FleetJoined {
        guard let id = Data(hex: fleet), id.count == 32 else { throw FleetError.encoding }
        let signature = try signer.sign(try bundle.joinMessage(fleet: id))
        var writer = FleetWriter()
        try bundle.write(&writer)
        let request = API.FleetJoinRequest(bundle: Base64URL.encode(writer.bytes), signature: Base64URL.encode(signature))
        let data = try await send("POST", "/api/v2/fleet/join", try JSONEncoder().encode(request))
        return try JSONDecoder().decode(API.FleetJoined.self, from: data)
    }
}
