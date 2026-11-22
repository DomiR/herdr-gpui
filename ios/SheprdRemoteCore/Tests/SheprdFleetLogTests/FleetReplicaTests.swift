import CryptoKit
import Foundation
import Testing
@testable import SheprdFleetLog
import SheprdRemoteCore

/// The phone's own copy, through the library the app links, checked
/// against the vector `crates/sheprd-fleet-ffi` writes (`replica_vector`).
struct FleetReplicaTests {
    struct Vector {
        let fleet: String
        let me: String
        let key: P256.Signing.PrivateKey
        let now: Date
        let ops: [String]
        let removedMember: String
        let host: String
        let expect: [String: Any]

        init() throws {
            let url = try #require(Bundle.module.url(forResource: "replica", withExtension: "json", subdirectory: "Fixtures"))
            let object = try #require(JSONSerialization.jsonObject(with: Data(contentsOf: url)) as? [String: Any])
            fleet = try #require(object["fleet"] as? String)
            me = try #require(object["me"] as? String)
            let hex = try #require(object["phone_key"] as? String)
            let raw = try #require(Data(hexString: hex))
            key = try P256.Signing.PrivateKey(rawRepresentation: raw)
            let ms = try #require(object["now_ms"] as? Double)
            now = Date(timeIntervalSince1970: ms / 1000)
            ops = try #require(object["ops"] as? [String])
            removedMember = try #require(object["removed_member"] as? String)
            host = try #require(object["host"] as? String)
            expect = try #require(object["expect"] as? [String: Any])
        }

        func replica() throws -> FleetReplica {
            try FleetReplica.apply(fleet: fleet, me: me, log: nil, ops: ops, at: now)
        }

        func sign(_ message: Data) throws -> Data {
            try key.signature(for: message).rawRepresentation
        }
    }

    @Test func aCopyStartsFromTheFleetItJoinedAndSaysWhatThePhoneSignsOn() throws {
        let vector = try Vector()
        let replica = try vector.replica()
        #expect(replica.status.heads == vector.expect["heads"] as? [String])
        let next = try #require(vector.expect["next"] as? [String: Any])
        #expect(replica.status.next?.seq == next["seq"] as? UInt64)
        #expect(replica.status.next?.parents == next["parents"] as? [String])
        #expect(replica.status.view?.members.count == vector.expect["members"] as? Int)
        #expect(replica.status.join == .member && !replica.removed)
        #expect(replica.status.view?.hosts.contains { $0.id == vector.host } == true)
        // Read back from its bytes, it says the same.
        let reopened = try FleetReplica.open(fleet: vector.fleet, me: vector.me, log: replica.log, at: vector.now)
        #expect(reopened.status == replica.status)
    }

    @Test func aDraftIsTheLibrarysBytesAndAppliesOnceSigned() throws {
        let vector = try Vector()
        let replica = try vector.replica()
        let drafts = try replica.draft([.remove(member: vector.removedMember, reason: .lost)], issued: vector.now, at: vector.now)
        #expect(drafts.map(\.signed) == [vector.expect["draft"] as? String])
        #expect(drafts.map(\.id) == [vector.expect["draft_id"] as? String])
        let (ops, sealed) = try replica.signing(drafts, with: vector.sign, at: vector.now)
        #expect(ops.count == 1 && sealed.applied == 1)
        #expect(sealed.status.view?.members.contains { $0.id == vector.removedMember } == false)
        #expect(sealed.status.next?.seq == 2)
        // Its own operation is what a computer at the old heads lacks.
        let missing = try sealed.missing(for: replica.status.heads, at: vector.now)
        #expect(missing.ops == ops && !missing.more)
    }

    @Test func whatTheReplayRefusesIsRefusedBeforeSigning() throws {
        let vector = try Vector()
        let replica = try vector.replica()
        let (_, sealed) = try replica.signing(
            replica.draft([.remove(member: vector.removedMember, reason: .lost)], issued: vector.now, at: vector.now),
            with: vector.sign, at: vector.now
        )
        // That member is gone: removing it again is refused, unsigned.
        var asked = false
        #expect(throws: FleetLogError.refused(rule: "unknown")) {
            let drafts = try sealed.draft([.remove(member: vector.removedMember, reason: .lost)], issued: vector.now, at: vector.now)
            _ = try sealed.signing(drafts, with: { _ in
                asked = true
                return Data(count: 64)
            }, at: vector.now)
        }
        #expect(!asked)
    }

    @Test func onlyThisPhonesOwnSignatureIsSealed() throws {
        let vector = try Vector()
        let replica = try vector.replica()
        let drafts = try replica.draft([.hostRemove(host: vector.host)], issued: vector.now, at: vector.now)
        let other = P256.Signing.PrivateKey()
        #expect(throws: FleetLogError.refused(rule: "signature")) {
            _ = try replica.signing(drafts, with: { try other.signature(for: $0).rawRepresentation }, at: vector.now)
        }
    }

    @Test func anotherHistoryOrAChangedFileStartsNothing() throws {
        let vector = try Vector()
        #expect(throws: FleetLogError.unfounded) {
            _ = try FleetReplica.apply(fleet: vector.fleet, me: vector.me, log: nil, ops: Array(vector.ops.dropFirst()), at: vector.now)
        }
        let replica = try vector.replica()
        var changed = replica.log
        changed[changed.count - 1] ^= 1
        do {
            _ = try FleetReplica.open(fleet: vector.fleet, me: vector.me, log: changed, at: vector.now)
            Issue.record("a changed log opened")
        } catch let error as FleetLogError {
            #expect(error.unreadable)
        }
        let otherFleet = String(repeating: "07", count: 32)
        #expect(throws: FleetLogError.otherFleet) {
            _ = try FleetReplica.open(fleet: otherFleet, me: vector.me, log: replica.log, at: vector.now)
        }
    }
}

extension Data {
    init?(hexString: String) {
        guard hexString.count.isMultiple(of: 2) else { return nil }
        var bytes = [UInt8]()
        var index = hexString.startIndex
        while index < hexString.endIndex {
            let next = hexString.index(index, offsetBy: 2)
            guard let byte = UInt8(hexString[index..<next], radix: 16) else { return nil }
            bytes.append(byte)
            index = next
        }
        self.init(bytes)
    }
}
