import Foundation
import Synchronization
import Testing
@testable import SheprdFleetLog
import SheprdRemoteCore

/// Syncing the phone's copy with computers of the fleet. Each fake computer
/// keeps its log with the same library, so it refuses what a daemon would.
struct FleetLogSyncTests {
    typealias Vector = FleetReplicaTests.Vector

    /// A computer that answers `POST /api/v2/fleet/sync` from its own log.
    final class Computer: Sendable {
        let replica: Mutex<FleetReplica>
        let posts = Mutex(0)
        let offline = Mutex(false)
        /// Takes nothing posted to it, as a computer missing its history.
        let ignoring = Mutex(false)

        init(_ replica: FleetReplica) {
            self.replica = Mutex(replica)
        }

        var client: FleetClient {
            FleetClient { _, path, body in
                guard path == "/api/v2/fleet/sync" else { throw URLError(.badURL) }
                self.posts.withLock { $0 += 1 }
                let request = try JSONDecoder().decode(Posted.self, from: body ?? Data())
                return try self.replica.withLock { replica in
                    guard request.fleet == replica.fleet else { throw URLError(.badServerResponse) }
                    let now = Date(timeIntervalSince1970: 1_800_000_000)
                    if !self.ignoring.withLock({ $0 }) {
                        replica = try replica.applying(request.ops, at: now)
                    }
                    // The answer to a post is lost on its way back.
                    if self.offline.withLock({ $0 }) { throw URLError(.networkConnectionLost) }
                    let missing = try replica.missing(for: request.heads, at: now)
                    let answer: [String: Any] = [
                        "fleet": replica.fleet, "heads": replica.status.heads, "ops": missing.ops,
                        "refused": replica.refused, "more": missing.more,
                    ]
                    return try JSONSerialization.data(withJSONObject: answer)
                }
            }
        }

        struct Posted: Decodable { let fleet: String; let heads: [String]; let ops: [String] }
    }

    @Test func aPhoneWithNoCopyStartsOneFromAnyComputer() async throws {
        let vector = try Vector()
        let now = vector.now
        let computer = Computer(try vector.replica())
        let outcome = try await FleetLogSync.run(fleet: vector.fleet, me: vector.me, replica: nil, with: computer.client) { now }
        #expect(outcome.replica.status.heads == outcome.heads)
        #expect(outcome.replica.status.join == .member)
        #expect(outcome.refused == 0)
    }

    @Test func whatThePhoneSignedReachesAComputerThatNeverSawThePhonesFirst() async throws {
        let vector = try Vector()
        let now = vector.now
        let replica = try vector.replica()
        let (_, signed) = try replica.signing(
            replica.draft([.remove(member: vector.removedMember, reason: .lost)], issued: vector.now, at: vector.now),
            with: vector.sign, at: vector.now
        )
        // The computer that introduced the phone is gone; it lost the
        // answer to the one post that reached it.
        let introducer = Computer(try vector.replica())
        introducer.offline.withLock { $0 = true }
        await #expect(throws: URLError.self) {
            _ = try await FleetLogSync.run(fleet: vector.fleet, me: vector.me, replica: signed, with: introducer.client) { now }
        }
        // Another computer of the fleet gets the very same bytes from the
        // phone's own copy: nothing is signed again.
        let other = Computer(try vector.replica())
        let outcome = try await FleetLogSync.run(fleet: vector.fleet, me: vector.me, replica: signed, with: other.client) { now }
        #expect(outcome.refused == 0 && !outcome.undelivered)
        #expect(outcome.heads == signed.status.heads)
        let held = other.replica.withLock { $0 }
        #expect(held.status.view?.members.contains { $0.id == vector.removedMember } == false)
    }

    @Test func aComputerOfAnotherFleetIsRefused() async throws {
        let vector = try Vector()
        let now = vector.now
        let stranger = FleetClient { _, _, _ in
            Data(#"{"fleet":"\#(String(repeating: "07", count: 32))","heads":[],"ops":[],"refused":0,"more":false}"#.utf8)
        }
        await #expect(throws: FleetLogSyncError.otherFleet) {
            _ = try await FleetLogSync.run(fleet: vector.fleet, me: vector.me, replica: nil, with: stranger) { now }
        }
    }

    @Test func aComputerThatHandsAnotherHistoryStartsNothing() async throws {
        // It names this fleet but sends history without its founding op.
        let vector = try Vector()
        let now = vector.now
        let ops = Array(vector.ops.dropFirst())
        let fleet = vector.fleet
        let liar = FleetClient { _, _, _ in
            let answer: [String: Any] = ["fleet": fleet, "heads": [], "ops": ops, "refused": 0, "more": false]
            return try JSONSerialization.data(withJSONObject: answer)
        }
        await #expect(throws: FleetLogError.unfounded) {
            _ = try await FleetLogSync.run(fleet: vector.fleet, me: vector.me, replica: nil, with: liar) { now }
        }
    }

    @Test func aBadAnswerFailsTheSyncAndLeavesTheCopy() async throws {
        let vector = try Vector()
        let now = vector.now
        let replica = try vector.replica()
        let fleet = vector.fleet
        // One operation that is not an operation.
        let garbage = FleetClient { _, _, _ in
            let answer: [String: Any] = ["fleet": fleet, "heads": [], "ops": ["!"], "refused": 0, "more": false]
            return try JSONSerialization.data(withJSONObject: answer)
        }
        await #expect(throws: FleetLogError.self) {
            _ = try await FleetLogSync.run(fleet: fleet, me: vector.me, replica: replica, with: garbage) { now }
        }
        // Too many to be one answer.
        let flood = FleetClient { _, _, _ in
            let ops = [String](repeating: "AA", count: FleetLogSync.maxAnswerOps + 1)
            let answer: [String: Any] = ["fleet": fleet, "heads": [], "ops": ops, "refused": 0, "more": true]
            return try JSONSerialization.data(withJSONObject: answer)
        }
        await #expect(throws: FleetLogSyncError.bounds) {
            _ = try await FleetLogSync.run(fleet: fleet, me: vector.me, replica: replica, with: flood) { now }
        }
        // The copy the phone holds is the one it had.
        let reopened = try FleetReplica.open(fleet: fleet, me: vector.me, log: replica.log, at: now)
        #expect(reopened.status == replica.status)
    }

    @Test func aDamagedCopyGivesBackThePhonesOwnOperations() throws {
        let vector = try Vector()
        let replica = try vector.replica()
        let (ops, signed) = try replica.signing(
            replica.draft([.remove(member: vector.removedMember, reason: .lost)], issued: vector.now, at: vector.now),
            with: vector.sign, at: vector.now
        )
        #expect(try FleetReplica.salvage(me: vector.me, log: signed.log) == ops)
        #expect(try FleetReplica.salvage(me: vector.me, log: Data("not a log".utf8)).isEmpty)
        // Started again from a computer's history, they apply once more.
        let again = try FleetReplica.apply(fleet: vector.fleet, me: vector.me, log: nil, ops: vector.ops + ops, at: vector.now)
        #expect(again.status.next?.seq == 2)
    }

    @Test func whatAComputerDidNotTakeIsUndelivered() async throws {
        let vector = try Vector()
        let now = vector.now
        let replica = try vector.replica()
        let (_, signed) = try replica.signing(
            replica.draft([.hostRemove(host: vector.host)], issued: vector.now, at: vector.now),
            with: vector.sign, at: vector.now
        )
        let computer = Computer(try vector.replica())
        computer.ignoring.withLock { $0 = true }
        let outcome = try await FleetLogSync.run(fleet: vector.fleet, me: vector.me, replica: signed, with: computer.client) { now }
        #expect(outcome.undelivered)
        // The phone's copy keeps it for the next computer.
        #expect(outcome.replica.status.heads == signed.status.heads)
    }
}
