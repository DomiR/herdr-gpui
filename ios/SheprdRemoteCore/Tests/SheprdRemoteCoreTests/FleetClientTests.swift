import CryptoKit
import Foundation
import Synchronization
import Testing
@testable import SheprdRemoteCore

/// The phone's side of the fleet: an operation's bytes checked against the
/// daemon's vector (`web/tests/fleet.rs`, `phone_vector`), and the calls
/// `FleetClient` makes.
struct FleetClientTests {
    /// Requests a fake computer saw, and what it answers.
    final class Computer: Sendable {
        let requests = Mutex<[(method: String, path: String, body: Data?)]>([])
        let answer: @Sendable (String, String, Data?) throws -> Data

        init(answer: @escaping @Sendable (String, String, Data?) throws -> Data) {
            self.answer = answer
        }

        var send: FleetClient.Send {
            { method, path, body in
                self.requests.withLock { $0.append((method, path, body)) }
                return try self.answer(method, path, body)
            }
        }

        var client: FleetClient { FleetClient(send: send) }
    }

    static func signer(_ key: P256.Signing.PrivateKey = P256.Signing.PrivateKey()) -> Fleet.Signer {
        Fleet.Signer(publicKey: key.publicKey.compressedRepresentation) { message in
            try key.signature(for: message).rawRepresentation
        }
    }

    static func status(next: API.FleetNext?) -> Data {
        var object: [String: Any] = [
            "view": [
                "fleet": String(repeating: "09", count: 32),
                "me": String(repeating: "aa", count: 32),
                "members": [["id": String(repeating: "aa", count: 32), "name": "studio", "kind": "computer"]],
                "hosts": [[
                    "id": String(repeating: "04", count: 32), "name": "studio", "kind": "member", "user": "me",
                    "added_by": String(repeating: "aa", count: 32), "grants": [], "relays": ["https://relay"], "rev": 1,
                ]],
                "removed": 0, "pending": 0,
            ],
            "heads": [String(repeating: "01", count: 32)],
            "reach": [["host": String(repeating: "04", count: 32), "direct": ["192.168.1.20:7443"]]],
        ]
        if let next {
            object["next"] = ["fleet": next.fleet, "seq": next.seq, "parents": next.parents]
            object["join"] = "member"
        }
        return try! JSONSerialization.data(withJSONObject: object)
    }

    static let applied = Data(#"{"fleet":"09","heads":[],"ops":[],"refused":0}"#.utf8)

    @Test func aLinkBuiltFromTheStatusIsTheDaemons() throws {
        let fixture = try FleetTests.fixture("phone")
        let next = try #require(fixture["next"] as? [String: Any])
        let draft = API.FleetNext(
            fleet: try #require(next["fleet"] as? String),
            seq: try #require(next["seq"] as? UInt64),
            parents: try #require(next["parents"] as? [String])
        )
        let deviceKey = try FleetTests.hex(fixture["device_key"])
        let op = try Fleet.draft(
            on: draft,
            signer: try FleetTests.hex(fixture["signer"]),
            issuedMs: try #require(fixture["issued_ms"] as? UInt64),
            body: .link(host: try FleetTests.hex(fixture["host"]), deviceKey: deviceKey)
        )
        #expect(try op.signedBytes() == FleetTests.hex(fixture["signed"]))
        let text = try #require(fixture["op"] as? String)
        let signed = try Fleet.Op.decode(try #require(Base64URL.decode(text)))
        #expect(signed.verify(key: try FleetTests.hex(fixture["signer_key"])))
        #expect(try signed.signedBytes() == op.signedBytes())
        #expect(Fleet.linkDevice(key: deviceKey) == fixture["device"] as? String)
        let compressed = try P256.Signing.PublicKey(x963Representation: deviceKey).compressedRepresentation
        #expect(Fleet.linkDevice(key: compressed) == fixture["device"] as? String)
        #expect(Fleet.linkDevice(key: Data(count: 65)) == nil)
    }

    @Test func aPhonesBundleHasNoSSH() throws {
        let key = P256.Signing.PrivateKey()
        let bundle = try Fleet.phoneBundle(
            key: key.publicKey.compressedRepresentation, name: "Pen\u{7}'s iPhone", relay: RelayKeyPair()
        )
        #expect(bundle.kind == .phone)
        #expect(bundle.ssh == nil && bundle.host == nil)
        #expect(bundle.name == "Pen's iPhone")
        var writer = FleetWriter()
        try bundle.write(&writer)
        var reader = FleetReader(writer.bytes)
        #expect(try Fleet.Bundle.read(&reader) == bundle)
    }

    struct Posted: Decodable { let fleet: String; let heads: [String]; let ops: [String] }

    @Test func aSyncNamesItsFleetAndIsBounded() async throws {
        let fleet = String(repeating: "09", count: 32)
        let computer = Computer { _, _, _ in
            Data(#"{"fleet":"\#(fleet)","heads":["01"],"ops":["b3A"],"refused":0,"more":true}"#.utf8)
        }
        let answer = try await computer.client.sync(["b3A"], heads: ["02"], fleet: fleet)
        #expect(answer.ops == ["b3A"] && answer.more)
        let request = try #require(computer.requests.withLock { $0.first })
        #expect(request.method == "POST" && request.path == "/api/v2/fleet/sync")
        let posted = try JSONDecoder().decode(Posted.self, from: try #require(request.body))
        #expect(posted.fleet == fleet && posted.heads == ["02"] && posted.ops == ["b3A"])
        // More than one sync carries is refused before anything is sent.
        let many = [String](repeating: "b3A", count: FleetClient.maxSyncOps + 1)
        await #expect(throws: FleetError.bounds) { try await computer.client.sync(many, heads: [], fleet: fleet) }
        let heads = [String](repeating: "01", count: FleetClient.maxSyncHeads + 1)
        await #expect(throws: FleetError.bounds) { try await computer.client.sync([], heads: heads, fleet: fleet) }
        #expect(computer.requests.withLock { $0.count } == 1)
    }

    @Test func aJoinCarriesTheBundleAndItsSignature() async throws {
        let key = P256.Signing.PrivateKey()
        let signer = Self.signer(key)
        let bundle = try Fleet.phoneBundle(key: signer.publicKey, name: "iPhone", relay: RelayKeyPair())
        let fleet = String(repeating: "09", count: 32)
        let computer = Computer { _, _, _ in
            Data(#"{"member":"\#(signer.member.hexString)","state":"pending"}"#.utf8)
        }
        let joined = try await computer.client.join(bundle, fleet: fleet, as: signer)
        #expect(joined.state == .pending)
        let body = try #require(computer.requests.withLock { $0.first?.body })
        let request = try #require(JSONSerialization.jsonObject(with: body) as? [String: String])
        let bundleText = try #require(request["bundle"])
        var reader = FleetReader(try #require(Base64URL.decode(bundleText)))
        #expect(try Fleet.Bundle.read(&reader) == bundle)
        let signatureText = try #require(request["signature"])
        let signature = try P256.Signing.ECDSASignature(rawRepresentation: try #require(Base64URL.decode(signatureText)))
        let message = try bundle.joinMessage(fleet: Data(repeating: 9, count: 32))
        #expect(key.publicKey.isValidSignature(signature, for: message))
    }

    @Test func theStatusReadsAndNamesWhereHostsAre() throws {
        let status = try JSONDecoder().decode(API.FleetStatus.self, from: Self.status(next: nil))
        #expect(status.view?.hosts.first?.runsSheprd == true)
        #expect(status.next == nil && status.join == nil && status.links.isEmpty)
        #expect(status.origins(of: String(repeating: "04", count: 32)) == ["https://192.168.1.20:7443"])
        #expect(status.origins(of: "other").isEmpty)
        let empty = try JSONDecoder().decode(API.FleetStatus.self, from: Data(#"{"heads":[]}"#.utf8))
        #expect(empty.view == nil)
    }
}
