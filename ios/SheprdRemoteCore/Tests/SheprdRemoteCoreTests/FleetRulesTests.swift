import CryptoKit
import Foundation
import Testing
import SheprdRemoteCore

/// The Swift copy refuses what the daemon refuses, and an app can build
/// its own payloads through the public initializers.
struct FleetRulesTests {
    @Test func everyInvalidVectorIsRefused() throws {
        let vectors = try #require(try FleetTests.fixture("invalid")["ops"] as? [[String: Any]])
        #expect(!vectors.isEmpty)
        for vector in vectors {
            let bytes = try FleetTests.hex(vector["op"])
            #expect(throws: FleetError.self, "\(vector["case"] ?? "")") { try Fleet.Op.decode(bytes) }
        }
    }

    @Test func aPhoneBundleMadeAComputerCannotBeSigned() throws {
        let vectors = try #require(try FleetTests.fixture("ops")["ops"] as? [[String: Any]])
        var op = try Fleet.Op.decode(try FleetTests.hex(vectors[2]["op"]))
        guard case let .admit(bundle, method, signature) = op.body else {
            Issue.record("vector 2 is not an admission")
            return
        }
        var computer = bundle
        computer.kind = .computer
        op.body = .admit(computer, method: method, joinerSignature: signature)
        #expect(throws: FleetError.invalid) { try op.sign(with: P256.Signing.PrivateKey()) }
        var blank = bundle
        blank.name = "   "
        #expect(throws: FleetError.invalid) { try blank.joinMessage(fleet: Data(count: 32)) }
    }

    @Test func aFoundingIsTheOnlyOperationWithoutParents() throws {
        let vectors = try #require(try FleetTests.fixture("ops")["ops"] as? [[String: Any]])
        var op = try Fleet.Op.decode(try FleetTests.hex(vectors[4]["op"]))
        op.parents = []
        #expect(throws: FleetError.invalid) { try op.signedBytes() }
    }

    @Test func payloadsAreBuiltThroughThePublicInitializers() throws {
        let ca = P256.Signing.PrivateKey()
        let caKey = ca.publicKey.x963Representation
        let authority = Fleet.CA(certificate: Data("cert".utf8) + caKey, key: caKey)
        var record = Fleet.SelfRecord(
            host: Data(SHA256.hash(data: authority.certificate)), rev: 1, route: "route",
            relays: ["https://relay.example.org"], identity: "identity", direct: [],
            sshHostKeys: ["ssh-ed25519 AAAA"], signedMs: 1
        )
        let intro = Fleet.HostIntro(
            kind: .member, name: "studio", user: "me", ca: authority, record: record,
            recordSignature: try ca.signature(for: try record.message()).rawRepresentation,
            verified: .bluetoothCode
        )
        let key = P256.Signing.PrivateKey()
        let bundle = Fleet.Bundle(
            key: key.publicKey.compressedRepresentation, kind: .computer, name: "studio",
            dial: P256.Signing.PrivateKey().publicKey.compressedRepresentation,
            tunnel: Data(count: 32), ssh: "ssh-ed25519 AAAA", host: intro
        )
        #expect(!(try bundle.joinMessage(fleet: Data(count: 32))).isEmpty)
        // A record for another host is not this intro's.
        record.host = Data(count: 32)
        var moved = intro
        moved.record = record
        var other = bundle
        other.host = moved
        #expect(throws: FleetError.invalid) { try other.joinMessage(fleet: Data(count: 32)) }
    }
}
