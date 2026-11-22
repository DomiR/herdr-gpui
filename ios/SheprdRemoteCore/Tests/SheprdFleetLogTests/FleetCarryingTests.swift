import Foundation
import Testing
@testable import SheprdFleetLog
import SheprdRemoteCore

/// Taking the phone's own operations back into a copy that may lack their
/// history, as one started again from a computer behind the rest.
struct FleetCarryingTests {
    typealias Vector = FleetReplicaTests.Vector

    /// The phone's removal, signed on the whole fleet's history.
    func signedRemoval(_ vector: Vector) throws -> [String] {
        let replica = try vector.replica()
        let drafts = try replica.draft([.remove(member: vector.removedMember, reason: .lost)], issued: vector.now, at: vector.now)
        return try replica.signing(drafts, with: vector.sign, at: vector.now).ops
    }

    @Test func whatWaitsForHistoryTheCopyLacksIsGivenBack() throws {
        let vector = try Vector()
        let ops = try signedRemoval(vector)
        // A copy from a computer that never saw the fleet's latest operation.
        let lagging = try FleetReplica.apply(
            fleet: vector.fleet, me: vector.me, log: nil, ops: Array(vector.ops.dropLast()), at: vector.now
        )
        let carried = try lagging.carrying(ops, at: vector.now)
        #expect(carried.replica.pending == 1)
        #expect(carried.waiting == ops)
        // The saved copy does not hold it: only what the caller keeps does.
        #expect(try FleetReplica.salvage(me: vector.me, log: carried.replica.log).isEmpty)
        // Once a sync brings that history, it applies and nothing waits.
        let caught = try carried.replica.applying(vector.ops, at: vector.now)
        let again = try caught.carrying(carried.waiting, at: vector.now)
        #expect(again.waiting.isEmpty && again.replica.applied == 1)
        #expect(again.replica.status.next?.seq == 2)
    }

    @Test func whatAppliesOrIsHeldIsNotGivenBack() throws {
        let vector = try Vector()
        let ops = try signedRemoval(vector)
        let carried = try vector.replica().carrying(ops, at: vector.now)
        #expect(carried.waiting.isEmpty && carried.replica.applied == 1)
        // Taken in again, it is already held.
        let held = try carried.replica.carrying(ops, at: vector.now)
        #expect(held.waiting.isEmpty && held.replica.pending == 0)
    }
}
