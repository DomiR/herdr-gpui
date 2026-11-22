import CryptoKit
import Foundation

/// The fleet calls a phone makes on a member computer's daemon
/// (`crates/sheprd-remote-control/src/web/fleet.rs`). Operations travel
/// as their canonical bytes in base64url; the JSON around them is a view,
/// never what is signed.
extension API {
    /// `GET /api/v2/fleet?member=ID`.
    public struct FleetStatus: Decodable, Equatable, Sendable {
        /// Nil while the computer is in no fleet.
        public let view: FleetView?
        public let heads: [String]
        /// What this phone signs its next operation on, once it is a member.
        public let next: FleetNext?
        /// Where this phone's join stands, when it asked here.
        public let join: FleetStanding?
        /// The member this answer was asked about (`FleetClient.status`),
        /// hex: its `next` and `join` speak for that key alone, so an
        /// answer about another key is never taken for the current one.
        public internal(set) var member: String?
        /// The hosts this phone linked a device key for, by id.
        public let links: [String]
        /// The device key each of those links names, base64url, by host.
        public let linkKeys: [String: String]
        /// Where hosts take direct connections, by host id.
        public let reach: [FleetReach]
        /// How to reach each Sheprd host this phone linked through its
        /// relays, from the host's own signed record.
        public let routes: [FleetRoute]

        public init(
            view: FleetView?, heads: [String] = [], next: FleetNext? = nil,
            join: FleetStanding? = nil, links: [String] = [], linkKeys: [String: String] = [:],
            reach: [FleetReach] = [], routes: [FleetRoute] = []
        ) {
            self.view = view
            self.heads = heads
            self.next = next
            self.join = join
            self.links = links
            self.linkKeys = linkKeys
            self.reach = reach
            self.routes = routes
        }

        enum CodingKeys: String, CodingKey { case view, heads, next, join, links, reach, routes
            case linkKeys = "link_keys"
        }

        public init(from decoder: Decoder) throws {
            let c = try decoder.container(keyedBy: CodingKeys.self)
            view = try c.decodeIfPresent(FleetView.self, forKey: .view)
            heads = try c.decodeIfPresent([String].self, forKey: .heads) ?? []
            next = try c.decodeIfPresent(FleetNext.self, forKey: .next)
            join = try c.decodeIfPresent(FleetStanding.self, forKey: .join)
            links = try c.decodeIfPresent([String].self, forKey: .links) ?? []
            linkKeys = try c.decodeIfPresent([String: String].self, forKey: .linkKeys) ?? [:]
            reach = try c.decodeIfPresent([FleetReach].self, forKey: .reach) ?? []
            routes = try c.decodeIfPresent([FleetRoute].self, forKey: .routes) ?? []
        }

        /// Where `host` is, from its own record once checked
        /// (`FleetRoute.verified`): nil unless the host is linked and its
        /// certificate authority, whose hash is the host id this phone
        /// pins, signed the record.
        public func relay(for host: String) -> FleetRecord? {
            routes.first { $0.host.lowercased() == host.lowercased() }?.verified()
        }

        /// How this phone reaches `host` through its relays with `keys`, its
        /// fleet bundle's, or `saved`'s when it has any: nil unless
        /// `relay(for:)` checks out and names relays, or when `saved`
        /// came from a record at least as new. A
        /// record's `rev` only goes up, so a computer serving an older one
        /// again cannot roll the phone back.
        public func relayRecord(for host: String, keys: RelayKeyPair, replacing saved: RelayRecord? = nil) -> RelayRecord? {
            guard let checked = relay(for: host) else { return nil }
            if let savedRev = saved?.signedRev, checked.rev <= savedRev { return nil }
            // Keys the phone already reaches the host with stay: it may have
            // sent its own since (`POST /api/relay`), and the host keeps
            // those over the bundle's.
            let keys = saved.map { RelayKeyPair(dialKey: $0.dialKey, tunnelKey: $0.tunnelKey) } ?? keys
            guard var record = RelayRecord.make(from: checked.relay, keys: keys) else { return nil }
            record.signedRev = checked.rev
            record.signedMs = checked.signedMs
            return record
        }

        /// Whether `member` is no longer in `fleet` as this computer sees
        /// it: neither a member nor waiting to join. Removed, or this
        /// computer has not heard of its admission yet.
        public func lacks(member: String, in fleet: String) -> Bool {
            guard let view, view.fleet.lowercased() == fleet.lowercased() else { return false }
            return join == nil && next == nil
                && !view.members.contains { $0.id.lowercased() == member.lowercased() }
        }

        /// The device key this phone's link names for each host it linked,
        /// by host id in lowercase.
        public var linkedKeys: [String: Data] {
            Dictionary(linkKeys.compactMap { host, key in
                Base64URL.decode(key).map { (host.lowercased(), $0) }
            }, uniquingKeysWith: { first, _ in first })
        }

        /// Whether this answer was asked about `member` (hex), the phone's
        /// key now: one asked about an earlier key, as a join cancelled and
        /// made again, says nothing about it.
        public func isAbout(member: String?) -> Bool {
            self.member?.lowercased() == member?.lowercased()
        }

        /// The direct addresses `host` publishes, as `https://` origins.
        public func origins(of host: String) -> [String] {
            (reach.first { $0.host == host }?.direct ?? [])
                .map { "https://\($0)" }
                .filter(Computer.isOrigin)
        }
    }

    /// The fleet as one member computer sees it.
    public struct FleetView: Decodable, Equatable, Sendable {
        public let fleet: String
        /// The computer's own member id.
        public let me: String
        public let members: [FleetMember]
        public let hosts: [FleetHost]
        /// Members removed, ever.
        public let removed: Int
        /// Operations waiting for their parents.
        public let pending: Int
    }

    public struct FleetMember: Decodable, Equatable, Identifiable, Sendable {
        public let id: String
        public let name: String
        public let kind: Kind
        public let admittedBy: String?

        public enum Kind: String, Decodable, Sendable { case computer, phone }

        enum CodingKeys: String, CodingKey {
            case id, name, kind
            case admittedBy = "admitted_by"
        }
    }

    public struct FleetHost: Decodable, Equatable, Identifiable, Sendable {
        public let id: String
        public let name: String
        public let kind: Kind
        public let user: String
        public let addedBy: String
        public let relays: [String]
        public let held: Bool

        /// What runs there: a computer member, a Sheprd host, or a plain
        /// host with the relay agent, which a phone never reaches.
        public enum Kind: String, Decodable, Sendable { case member, sheprd, agent }

        /// A host whose daemon a phone can control.
        public var runsSheprd: Bool { kind != .agent }

        enum CodingKeys: String, CodingKey {
            case id, name, kind, user, relays, held
            case addedBy = "added_by"
        }

        public init(from decoder: Decoder) throws {
            let c = try decoder.container(keyedBy: CodingKeys.self)
            id = try c.decode(String.self, forKey: .id)
            name = try c.decode(String.self, forKey: .name)
            kind = try c.decode(Kind.self, forKey: .kind)
            user = try c.decode(String.self, forKey: .user)
            addedBy = try c.decode(String.self, forKey: .addedBy)
            relays = try c.decodeIfPresent([String].self, forKey: .relays) ?? []
            held = try c.decodeIfPresent(Bool.self, forKey: .held) ?? false
        }
    }

    public struct FleetNext: Decodable, Equatable, Sendable {
        public let fleet: String
        public let seq: UInt64
        public let parents: [String]

        public init(fleet: String, seq: UInt64, parents: [String]) {
            self.fleet = fleet
            self.seq = seq
            self.parents = parents
        }
    }

    public enum FleetStanding: String, Decodable, Sendable { case pending, member }

    /// A linked host's latest record as its certificate authority signed
    /// it, base64url. The computer that sends it is only a carrier: the
    /// phone checks it against the host id before using it.
    public struct FleetRoute: Decodable, Equatable, Sendable {
        public let host: String
        /// The CA's certificate, DER: its SHA-256 is the host's id.
        public let certificate: String
        /// The CA's key, SEC1 uncompressed, found inside the certificate.
        public let key: String
        /// What the CA signed: the fleet's domain, `/self`, and the record.
        public let record: String
        public let signature: String

        public init(host: String, certificate: String, key: String, record: String, signature: String) {
            self.host = host
            self.certificate = certificate
            self.key = key
            self.record = record
            self.signature = signature
        }

        /// The relays the record names, when the certificate hashes to
        /// `host`, holds the key, and that key signed a record for `host`;
        /// nil otherwise. The same checks a member makes on a host's
        /// `HostSeen` (`HostIntro.vouches`, `Ca::check`).
        public func verified() -> FleetRecord? {
            guard let id = Fleet.data(hex: host),
                  let certificate = Base64URL.decode(certificate),
                  let key = Base64URL.decode(key), key.count == 65, key.first == 0x04,
                  let message = Base64URL.decode(record),
                  let signature = Base64URL.decode(signature),
                  Data(SHA256.hash(data: certificate)) == id,
                  certificate.range(of: key) != nil,
                  let point = try? P256.Signing.PublicKey(x963Representation: key),
                  let parsed = try? P256.Signing.ECDSASignature(rawRepresentation: signature),
                  point.isValidSignature(parsed, for: message)
            else { return nil }
            let prefix = Fleet.domain + Data("/self".utf8)
            guard message.starts(with: prefix) else { return nil }
            var reader = FleetReader(message.dropFirst(prefix.count))
            guard let record = try? Fleet.SelfRecord.read(&reader), (try? reader.finish()) != nil,
                  record.host == id
            else { return nil }
            return FleetRecord(
                relay: RelayInfo(identity: record.identity, relays: record.relays, route: record.route),
                rev: record.rev, signedMs: record.signedMs
            )
        }
    }

    /// A linked host's relays from its checked record, with the record's
    /// `rev` and signing time.
    public struct FleetRecord: Equatable, Sendable {
        public let relay: RelayInfo
        public let rev: UInt64
        public let signedMs: UInt64
    }

    public struct FleetReach: Decodable, Equatable, Sendable {
        public let host: String
        public let direct: [String]
    }

    /// `POST /api/v2/fleet/sync`.
    public struct FleetSync: Encodable, Equatable, Sendable {
        /// The fleet the phone syncs: a computer of another refuses it.
        public let fleet: String
        public let heads: [String]
        public let ops: [String]
    }

    /// What a sync came to: the computer's heads, and what the phone lacks.
    public struct FleetApplied: Decodable, Equatable, Sendable {
        public let fleet: String
        public let heads: [String]
        public let ops: [String]
        public let refused: Int
        public let more: Bool

        enum CodingKeys: String, CodingKey { case fleet, heads, ops, refused, more }

        public init(from decoder: Decoder) throws {
            let c = try decoder.container(keyedBy: CodingKeys.self)
            fleet = try c.decode(String.self, forKey: .fleet)
            heads = try c.decode([String].self, forKey: .heads)
            ops = try c.decodeIfPresent([String].self, forKey: .ops) ?? []
            refused = try c.decodeIfPresent(Int.self, forKey: .refused) ?? 0
            more = try c.decodeIfPresent(Bool.self, forKey: .more) ?? false
        }
    }

    /// `POST /api/v2/fleet/join`.
    public struct FleetJoinRequest: Encodable, Equatable, Sendable {
        public let bundle: String
        public let signature: String
    }

    public struct FleetJoined: Decodable, Equatable, Sendable {
        public let member: String
        public let state: FleetStanding
    }
}
