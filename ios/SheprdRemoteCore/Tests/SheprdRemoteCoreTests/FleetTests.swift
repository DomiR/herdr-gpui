import CryptoKit
import Foundation
import Testing
@testable import SheprdRemoteCore

/// Checks the Swift copy against the vectors the daemon's Rust tests write
/// (`control/fleet/tests/vectors.rs`): every body reads, writes back byte
/// for byte, hashes to the same id, and its signature checks out.
struct FleetTests {
    static func fixture(_ name: String) throws -> [String: Any] {
        let url = try #require(Bundle.module.url(forResource: "Fixtures/fleet/\(name)", withExtension: "json"))
        return try #require(JSONSerialization.jsonObject(with: Data(contentsOf: url)) as? [String: Any])
    }

    static func hex(_ text: Any?) throws -> Data {
        let text = try #require(text as? String)
        var data = Data()
        var index = text.startIndex
        while index < text.endIndex {
            let next = text.index(index, offsetBy: 2)
            data.append(try #require(UInt8(text[index..<next], radix: 16)))
            index = next
        }
        return data
    }

    @Test func everyBodyReadsWritesAndVerifiesAsTheDaemons() throws {
        let fixture = try Self.fixture("ops")
        let vectors = try #require(fixture["ops"] as? [[String: Any]])
        var bodies = Set<String>()
        for vector in vectors {
            let bytes = try Self.hex(vector["op"])
            let op = try Fleet.Op.decode(bytes)
            #expect(try op.encode() == bytes)
            #expect(try op.id() == Self.hex(vector["id"]))
            #expect(op.signer == (try Self.hex(vector["signer"])))
            #expect(op.seq == (vector["seq"] as? UInt64))
            #expect(op.verify(key: try Self.hex(vector["signer_key"])))
            var forged = op
            forged.issuedMs += 1
            #expect(!forged.verify(key: try Self.hex(vector["signer_key"])))
            bodies.insert(try #require(vector["body"] as? String))
        }
        #expect(bodies.count == 10)
    }

    @Test func malformedOperationsAreRefused() throws {
        let vectors = try #require(try Self.fixture("ops")["ops"] as? [[String: Any]])
        let bytes = try Self.hex(vectors[1]["op"])
        let signed = bytes.prefix(bytes.count - 64)
        #expect(throws: FleetError.self) { try Fleet.Op.decode(signed + Data([0]) + bytes.suffix(64)) }
        #expect(throws: FleetError.self) { try Fleet.Op.decode(Data(bytes.dropFirst())) }
        var version = bytes
        version[Fleet.domain.count] = 2
        #expect(throws: FleetError.encoding) { try Fleet.Op.decode(version) }
        #expect(throws: FleetError.bounds) { try Fleet.Op.decode(Data(count: 16 * 1024 + 1)) }
    }

    @Test func aJoinerSignsTheSameMessage() throws {
        let fixture = try Self.fixture("signed")
        let join = try #require(fixture["join"] as? [String: Any])
        let message = try Self.hex(join["message"])
        let key = try P256.Signing.PublicKey(compressedRepresentation: try Self.hex(join["key"]))
        let signature = try P256.Signing.ECDSASignature(rawRepresentation: try Self.hex(join["signature"]))
        #expect(key.isValidSignature(signature, for: message))
        // The bundle inside the admission vector builds the same message.
        let vectors = try #require(try Self.fixture("ops")["ops"] as? [[String: Any]])
        let admissions = try vectors.map { try Fleet.Op.decode(try Self.hex($0["op"])) }.compactMap { op -> Fleet.Bundle? in
            if case let .admit(bundle, _, _) = op.body, bundle.kind == .phone { return bundle }
            return nil
        }
        let bundle = try #require(admissions.first)
        #expect(try bundle.joinMessage(fleet: try Self.hex(join["fleet"])) == message)
    }

    @Test func aHostsRecordIsCheckedAgainstItsCA() throws {
        let fixture = try Self.fixture("signed")
        let selfRecord = try #require(fixture["self_record"] as? [String: Any])
        let message = try Self.hex(selfRecord["message"])
        let key = try P256.Signing.PublicKey(x963Representation: try Self.hex(selfRecord["key"]))
        let signature = try P256.Signing.ECDSASignature(rawRepresentation: try Self.hex(selfRecord["signature"]))
        #expect(key.isValidSignature(signature, for: message))
        #expect(Data(SHA256.hash(data: try Self.hex(selfRecord["certificate"]))) == (try Self.hex(selfRecord["host"])))
        let plain = try #require(fixture["plain_host"] as? [String: Any])
        let first = try #require(plain["first_host_key"] as? String)
        let id = Data(SHA256.hash(data: Fleet.domain + Data("/host".utf8) + Data(first.utf8)))
        #expect(id == (try Self.hex(plain["id"])))
    }

    @Test func carriedRecordsVerifyThroughTheIntro() throws {
        let vectors = try #require(try Self.fixture("ops")["ops"] as? [[String: Any]])
        let ops = try vectors.map { try Fleet.Op.decode(try Self.hex($0["op"])) }
        var intros: [Data: Fleet.HostIntro] = [:]
        for op in ops {
            if case let .hostAdd(intro) = op.body, let id = intro.id { intros[id] = intro }
        }
        var checked = 0
        for op in ops {
            guard case let .hostSeen(record, signature) = op.body else { continue }
            let intro = try #require(intros[record.host])
            #expect(intro.vouches(for: record, signature: signature))
            var moved = record
            moved.rev += 1
            #expect(!intro.vouches(for: moved, signature: signature))
            checked += 1
        }
        #expect(checked == 1)
    }

    @Test func aPhoneSignsWhatTheDaemonVerifies() throws {
        let vectors = try #require(try Self.fixture("ops")["ops"] as? [[String: Any]])
        var op = try Fleet.Op.decode(try Self.hex(vectors[1]["op"]))
        let key = P256.Signing.PrivateKey()
        op.body = .hostRemove(Data(repeating: 7, count: 32))
        try op.sign(with: key)
        #expect(op.verify(key: key.publicKey.compressedRepresentation))
        #expect(try Fleet.Op.decode(try op.encode()) == op)
    }
}
