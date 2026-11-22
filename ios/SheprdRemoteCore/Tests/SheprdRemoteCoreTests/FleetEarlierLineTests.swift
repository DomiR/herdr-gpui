import Foundation
import Testing
@testable import SheprdRemoteCore

/// A record saved before the phone kept its own copy of the log loses
/// none of what it signed on an update.
struct FleetEarlierLineTests {
    static let me = Data(repeating: 0xAA, count: 32)
    static let other = Data(repeating: 0xBB, count: 32)

    static func op(_ seq: UInt64, by signer: Data = me) throws -> Data {
        try Fleet.Op(
            fleet: Data(repeating: 9, count: 32), signer: signer, seq: seq, issuedMs: 1,
            parents: [Data(repeating: 1, count: 32)], body: .hostRemove(Data(repeating: UInt8(seq), count: 32)),
            signature: Data(repeating: 7, count: 64)
        ).encode()
    }

    static func line(_ json: [String: Any]) throws -> FleetEarlierLine {
        try JSONDecoder().decode(FleetEarlierLine.self, from: JSONSerialization.data(withJSONObject: json))
    }

    @Test func aChainCarriesEveryOperationAndKeepsTheHighestThatMayStand() throws {
        let (last, waiting, refused) = (try Self.op(3), try Self.op(4), try Self.op(5))
        let line = try Self.line([
            "chain": [
                "last": last.base64EncodedString(),
                "unconfirmed": [waiting.base64EncodedString()],
                "refusal": ["ops": [refused.base64EncodedString()]],
            ],
        ])
        let (ops, signedSeq) = line.carried(member: Self.me.hexString, unconfirmed: nil)
        // The refused post is carried, so whatever of it stood is taken in,
        // but does not hold the next signature back.
        #expect(ops == [last, waiting, refused].map(Base64URL.encode))
        #expect(signedSeq == 4)
    }

    @Test func anOlderRecordCarriesItsLastSignedAndUnconfirmed() throws {
        let (last, waiting) = (try Self.op(2), try Self.op(3))
        let line = try Self.line(["lastSigned": Base64URL.encode(last)])
        let (ops, signedSeq) = line.carried(member: Self.me.hexString, unconfirmed: [Base64URL.encode(waiting)])
        #expect(ops == [last, waiting].map(Base64URL.encode))
        #expect(signedSeq == 3)
    }

    @Test func onlyThisMembersWellFormedOperationsAreCarriedOnce() throws {
        let mine = try Self.op(2)
        let line = try Self.line([
            "lastSigned": Base64URL.encode(mine),
            "chain": [
                "last": mine.base64EncodedString(),
                "unconfirmed": [try Self.op(9, by: Self.other).base64EncodedString(), Data([1, 2, 3]).base64EncodedString()],
            ],
        ])
        let (ops, signedSeq) = line.carried(member: Self.me.hexString.uppercased(), unconfirmed: ["not base64url!"])
        #expect(ops == [Base64URL.encode(mine)])
        #expect(signedSeq == 2)
    }

    @Test func aRecordWithNoEarlierLineCarriesNothing() throws {
        let (ops, signedSeq) = try Self.line([:]).carried(member: Self.me.hexString, unconfirmed: nil)
        #expect(ops.isEmpty && signedSeq == nil)
    }
}
