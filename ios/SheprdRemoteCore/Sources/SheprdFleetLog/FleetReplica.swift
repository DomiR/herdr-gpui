import Foundation
import SheprdRemoteCore

/// This phone's own copy of the fleet log (`docs/relay-plan.md`, Fleet,
/// phase 5): its bytes, kept by the app in its container, and what they
/// tell the phone, computed by the same Rust code every computer runs
/// (`crates/sheprd-fleet-ffi`). Every call checks the bytes again.
public struct FleetReplica: Sendable {
    /// The fleet this phone joined, hex: the hash of its founding
    /// operation, the only one a copy starts from.
    public let fleet: String
    /// This phone's member id, hex.
    public let me: String
    /// The saved log (`saved::encode`).
    public let log: Data
    /// What to name as heads when asking a computer for what the phone
    /// lacks: its heads, then its latest operations, so history the two
    /// share counts even when the phone's heads are new to the computer.
    public let known: [String]
    /// The fleet as the log says it, in the shape a computer answers.
    public let status: API.FleetStatus
    /// This phone was removed: it signs nothing more.
    public let removed: Bool
    /// The last call's operations that applied, wait for their parents, or
    /// were refused.
    public let applied: Int
    public let pending: Int
    public let refused: Int

    /// Takes `ops` (base64url) into `log`, or starts a copy from them when
    /// there is none: only from the operation that founded `fleet`.
    public static func apply(
        fleet: String, me: String, log: Data?, ops: [String], at now: Date
    ) throws -> FleetReplica {
        let request = Request.Apply(
            fleet: fleet, me: me, log: log.map(Base64URL.encode), ops: ops, nowMs: milliseconds(now)
        )
        return try FleetReplica(fleet: fleet, me: me, answer: FleetLogFFI.call(request))
    }

    /// Opens a saved copy, checking it again.
    public static func open(fleet: String, me: String, log: Data, at now: Date) throws -> FleetReplica {
        try apply(fleet: fleet, me: me, log: log, ops: [], at: now)
    }

    /// The operations `me` signed that saved `log` holds, read for their
    /// shape alone from a copy that no longer opens, so a copy started
    /// again takes them back, checking each. Nothing for a file that is
    /// not a log.
    public static func salvage(me: String, log: Data) throws -> [String] {
        let request = Request.Salvage(me: me, log: Base64URL.encode(log))
        return try JSONDecoder().decode(Salvaged.self, from: FleetLogFFI.call(request)).ops
    }

    /// This copy with `ops` taken in.
    public func applying(_ ops: [String], at now: Date) throws -> FleetReplica {
        try Self.apply(fleet: fleet, me: me, log: log, ops: ops, at: now)
    }

    /// This copy with `ops`, this phone's own, taken in, and those of them
    /// it still lacks because they wait for history it does not hold: a
    /// copy started again from a computer behind the rest, say. The saved
    /// log keeps no waiting operation, so the caller keeps these to take in
    /// again once a sync brings that history. None when nothing waits, as
    /// what else was not taken was refused, which no history changes.
    public func carrying(_ ops: [String], at now: Date) throws -> (replica: FleetReplica, waiting: [String]) {
        let merged = try applying(ops, at: now)
        guard merged.pending > 0 else { return (merged, []) }
        let held = Set(try Self.salvage(me: me, log: merged.log).compactMap(Base64URL.decode))
        let waiting = ops.filter { op in Base64URL.decode(op).map { !held.contains($0) } ?? false }
        return (merged, waiting)
    }

    /// What a computer whose heads are `heads` lacks: one sync's worth,
    /// and whether there is more.
    public func missing(for heads: [String], at now: Date) throws -> (ops: [String], more: Bool) {
        let request = Request.Missing(fleet: fleet, log: Base64URL.encode(log), heads: heads, nowMs: Self.milliseconds(now))
        let answer = try JSONDecoder().decode(Missing.self, from: FleetLogFFI.call(request))
        return (answer.ops, answer.more)
    }

    /// Drafts one operation per body on this copy's own heads and this
    /// phone's own sequence, each on the one before, and refuses them all,
    /// before anything is signed, unless each would apply.
    public func draft(_ bodies: [FleetDraft], issued: Date, at now: Date) throws -> [FleetDrafted] {
        let request = Request.Draft(
            fleet: fleet, me: me, log: Base64URL.encode(log), issuedMs: Self.milliseconds(issued),
            nowMs: Self.milliseconds(now), bodies: bodies
        )
        return try JSONDecoder().decode(Drafts.self, from: FleetLogFFI.call(request)).drafts
    }

    /// This copy with `ops`, which this phone just signed, added: each must
    /// apply, or none is.
    public func sealing(_ ops: [String], at now: Date) throws -> FleetReplica {
        let request = Request.Seal(fleet: fleet, me: me, log: Base64URL.encode(log), ops: ops, nowMs: Self.milliseconds(now))
        return try FleetReplica(fleet: fleet, me: me, answer: FleetLogFFI.call(request))
    }

    /// Signs `drafts` with `sign` (this phone's fleet key) and seals them:
    /// the operations, base64url, and the copy that holds them.
    public func signing(
        _ drafts: [FleetDrafted], with sign: (Data) throws -> Data, at now: Date
    ) throws -> (ops: [String], replica: FleetReplica) {
        let ops = try drafts.map { draft -> String in
            guard let signed = Base64URL.decode(draft.signed) else { throw FleetLogError.call }
            let signature = try sign(signed)
            guard signature.count == 64 else { throw FleetError.signature }
            return Base64URL.encode(signed + signature)
        }
        return (ops, try sealing(ops, at: now))
    }

    static func milliseconds(_ date: Date) -> UInt64 {
        UInt64(max(0, date.timeIntervalSince1970 * 1000))
    }

    private init(fleet: String, me: String, answer: Data) throws {
        let decoded = try JSONDecoder().decode(Answer.self, from: answer)
        guard let log = Base64URL.decode(decoded.log) else { throw FleetLogError.call }
        self.fleet = fleet
        self.me = me
        self.log = log
        known = decoded.known
        status = decoded.status
        removed = decoded.removed.removed
        applied = decoded.applied
        pending = decoded.pending
        refused = decoded.refused
    }

    private struct Answer: Decodable {
        let log: String
        let known: [String]
        let applied: Int
        let pending: Int
        let refused: Int
        let status: API.FleetStatus
        let removed: Removed

        enum CodingKeys: String, CodingKey { case log, known, applied, pending, refused, status }

        init(from decoder: Decoder) throws {
            let c = try decoder.container(keyedBy: CodingKeys.self)
            log = try c.decode(String.self, forKey: .log)
            known = try c.decode([String].self, forKey: .known)
            applied = try c.decode(Int.self, forKey: .applied)
            pending = try c.decode(Int.self, forKey: .pending)
            refused = try c.decode(Int.self, forKey: .refused)
            status = try c.decode(API.FleetStatus.self, forKey: .status)
            removed = try c.decode(Removed.self, forKey: .status)
        }
    }

    private struct Removed: Decodable { let removed: Bool }
    private struct Missing: Decodable { let ops: [String]; let more: Bool }
    private struct Salvaged: Decodable { let ops: [String] }
    private struct Drafts: Decodable { let drafts: [FleetDrafted] }

    private enum Request {
        struct Apply: Encodable {
            let call = "apply"
            let fleet: String
            let me: String
            let log: String?
            let ops: [String]
            let nowMs: UInt64
            enum CodingKeys: String, CodingKey { case call, fleet, me, log, ops, nowMs = "now_ms" }
        }

        struct Salvage: Encodable {
            let call = "salvage"
            let me: String
            let log: String
        }

        struct Missing: Encodable {
            let call = "missing"
            let fleet: String
            let log: String
            let heads: [String]
            let nowMs: UInt64
            enum CodingKeys: String, CodingKey { case call, fleet, log, heads, nowMs = "now_ms" }
        }

        struct Draft: Encodable {
            let call = "draft"
            let fleet: String
            let me: String
            let log: String
            let issuedMs: UInt64
            let nowMs: UInt64
            let bodies: [FleetDraft]
            enum CodingKeys: String, CodingKey {
                case call, fleet, me, log, bodies
                case issuedMs = "issued_ms", nowMs = "now_ms"
            }
        }

        struct Seal: Encodable {
            let call = "seal"
            let fleet: String
            let me: String
            let log: String
            let ops: [String]
            let nowMs: UInt64
            enum CodingKeys: String, CodingKey { case call, fleet, me, log, ops, nowMs = "now_ms" }
        }
    }
}

/// What this phone may sign.
public enum FleetDraft: Encodable, Equatable, Sendable {
    /// Removes `member` (hex), with what it introduced that the person
    /// picked.
    case remove(member: String, alsoMembers: [String] = [], alsoHosts: [String] = [], reason: Fleet.Reason)
    /// Removes a host (hex) from the whole fleet.
    case hostRemove(host: String)
    /// Links this phone to a Sheprd host (hex) with a device key (P-256,
    /// SEC1) made for it.
    case link(host: String, deviceKey: Data)

    enum CodingKeys: String, CodingKey {
        case body, member, host, reason
        case alsoMembers = "also_members", alsoHosts = "also_hosts", deviceKey = "device_key"
    }

    public func encode(to encoder: Encoder) throws {
        var c = encoder.container(keyedBy: CodingKeys.self)
        switch self {
        case let .remove(member, alsoMembers, alsoHosts, reason):
            try c.encode("remove", forKey: .body)
            try c.encode(member, forKey: .member)
            try c.encode(alsoMembers, forKey: .alsoMembers)
            try c.encode(alsoHosts, forKey: .alsoHosts)
            try c.encode(reason.name, forKey: .reason)
        case let .hostRemove(host):
            try c.encode("host_remove", forKey: .body)
            try c.encode(host, forKey: .host)
        case let .link(host, deviceKey):
            try c.encode("link", forKey: .body)
            try c.encode(host, forKey: .host)
            try c.encode(Base64URL.encode(deviceKey), forKey: .deviceKey)
        }
    }
}

/// One drafted operation: the bytes to sign, base64url, and its id.
public struct FleetDrafted: Decodable, Equatable, Sendable {
    public let signed: String
    public let id: String
    public let seq: UInt64
    public let parents: [String]
}

extension Fleet.Reason {
    /// The name the log's JSON uses.
    var name: String {
        switch self {
        case .lost: "lost"
        case .retired: "retired"
        case .compromised: "compromised"
        case .left: "left"
        }
    }
}
