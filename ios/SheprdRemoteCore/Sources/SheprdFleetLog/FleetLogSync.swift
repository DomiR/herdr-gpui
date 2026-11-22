import Foundation
import SheprdRemoteCore

/// One sync of this phone's copy with one computer of the fleet, over
/// `POST /api/v2/fleet/sync`: the phone takes what the computer has and it
/// lacks, checking every operation itself, then hands over what the
/// computer lacks, its own signed operations among them. Any computer the
/// phone reaches will do, so the phone carries operations between
/// computers that cannot reach each other, and needs no one computer.
public enum FleetLogSync {
    /// Rounds one sync takes each way, at most.
    public static let maxRounds = 16
    /// Operations one answer may carry: a computer's answer stops at 256
    /// KiB, and every operation is more than 100 bytes.
    public static let maxAnswerOps = 4096
    /// The base64url text of one answer's operations: 256 KiB, and room.
    public static let maxAnswerText = 512 * 1024

    public struct Outcome: Sendable {
        public let replica: FleetReplica
        /// The computer's heads after the sync.
        public let heads: [String]
        /// How many of this phone's operations the computer refused.
        public let refused: Int
        /// Operations this phone signed that the computer still lacks after
        /// the sync: refused, held waiting, or not yet sent. Only without
        /// any did they reach it.
        public let undelivered: Bool
    }

    /// Syncs `replica` (nil: no copy yet, which then starts from the
    /// computer's history, from the operation that founded `fleet` only)
    /// with the computer `client` reaches. Nothing a computer answers can
    /// make the phone drop its copy: a bad answer fails this sync alone.
    public static func run(
        fleet: String, me: String, replica: FleetReplica?, with client: FleetClient,
        now: @Sendable () -> Date = { Date() }
    ) async throws -> Outcome {
        var replica = replica
        var heads = try await pull(fleet: fleet, me: me, into: &replica, from: client, now: now, rounds: 1)
        guard var replica else { throw FleetLogError.unfounded }
        var refused = 0
        for _ in 0..<maxRounds {
            let (ops, _) = try replica.missing(for: heads, at: now())
            guard !ops.isEmpty else { break }
            let answer = try await client.sync(ops, heads: replica.known, fleet: fleet)
            try check(answer, fleet: fleet)
            refused += answer.refused
            replica = try replica.applying(answer.ops, at: now())
            // Nothing it took moved its heads: it refused them, or holds
            // them waiting. Sending them again changes nothing.
            if answer.heads == heads { break }
            heads = answer.heads
        }
        var pulled: FleetReplica? = replica
        heads = try await pull(fleet: fleet, me: me, into: &pulled, from: client, now: now, rounds: maxRounds)
        let final = pulled ?? replica
        let left = try final.missing(for: heads, at: now())
        let undelivered = left.more || left.ops.contains { text in
            Base64URL.decode(text).flatMap { try? Fleet.Op.decode($0) }?.signer.hexString == me.lowercased()
        }
        return Outcome(replica: final, heads: heads, refused: refused, undelivered: undelivered)
    }

    /// Takes what the computer has, `rounds` answers at most, stopping once
    /// it has no more or an answer brings nothing new. Answers its heads.
    private static func pull(
        fleet: String, me: String, into replica: inout FleetReplica?, from client: FleetClient,
        now: () -> Date, rounds: Int
    ) async throws -> [String] {
        var heads: [String] = []
        for _ in 0..<rounds {
            let answer = try await client.sync([], heads: replica.map(\.known) ?? [], fleet: fleet)
            try check(answer, fleet: fleet)
            heads = answer.heads
            let next = try replica.map { try $0.applying(answer.ops, at: now()) }
                ?? FleetReplica.apply(fleet: fleet, me: me, log: nil, ops: answer.ops, at: now())
            replica = next
            // A computer that does not know this phone's heads answers from
            // the start; what it sends again is no progress.
            guard answer.more, next.applied > 0 else { break }
        }
        return heads
    }

    private static func check(_ answer: API.FleetApplied, fleet: String) throws {
        guard answer.fleet.lowercased() == fleet.lowercased() else { throw FleetLogSyncError.otherFleet }
        guard answer.ops.count <= maxAnswerOps,
              answer.ops.reduce(0, { $0 + $1.utf8.count }) <= maxAnswerText
        else { throw FleetLogSyncError.bounds }
    }
}

/// Why a sync with one computer failed: its fault, never the copy's, so the
/// copy stays as it is.
public enum FleetLogSyncError: Error, Equatable, Sendable {
    /// It answered for another fleet.
    case otherFleet
    /// Its answer is over a bound.
    case bounds
}
