import CryptoKit
import Foundation
import Testing
@testable import SheprdRemoteCore

/// The relays a linked host is reached through: its own record, signed by
/// its certificate authority, which the computer that answers
/// `GET /api/v2/fleet` only carries. The phone checks it against the host
/// id it pins, as the daemon's vector (`signed.json`) and a record signed
/// here show, before making the relay record it keeps.
struct FleetRouteTests {
    /// A host's CA, and records it signs.
    struct Host {
        let key = P256.Signing.PrivateKey()
        var certificate: Data { Data("certificate:".utf8) + key.publicKey.x963Representation + Data(":end".utf8) }
        var id: Data { Data(SHA256.hash(data: certificate)) }

        func record(rev: UInt64 = 1, host: Data? = nil) -> Fleet.SelfRecord {
            let x25519 = Curve25519.KeyAgreement.PrivateKey().publicKey.rawRepresentation
            let mlkem = (try? MLKEM768.PrivateKey())?.publicKey.rawRepresentation ?? Data()
            return Fleet.SelfRecord(
                host: host ?? id, rev: rev, route: Base64URL.encode(Data(repeating: 0x52, count: 32)),
                relays: ["https://relay.example.org"], identity: Base64URL.encode(x25519 + mlkem),
                direct: [], sshHostKeys: ["ssh-ed25519 AAAA"], signedMs: 1
            )
        }

        func route(_ record: Fleet.SelfRecord, signedBy signer: P256.Signing.PrivateKey? = nil) throws -> API.FleetRoute {
            let message = try record.message()
            let signature = try (signer ?? key).signature(for: message).rawRepresentation
            return API.FleetRoute(
                host: id.hexString, certificate: Base64URL.encode(certificate),
                key: Base64URL.encode(key.publicKey.x963Representation),
                record: Base64URL.encode(message), signature: Base64URL.encode(signature)
            )
        }
    }

    static func status(_ routes: [API.FleetRoute]) -> API.FleetStatus {
        API.FleetStatus(view: nil, links: routes.map(\.host), routes: routes)
    }

    @Test func theDaemonsSignedRecordChecksOut() throws {
        let fixture = try #require(try FleetTests.fixture("signed")["self_record"] as? [String: Any])
        let route = API.FleetRoute(
            host: try #require(fixture["host"] as? String),
            certificate: Base64URL.encode(try FleetTests.hex(fixture["certificate"])),
            key: Base64URL.encode(try FleetTests.hex(fixture["key"])),
            record: Base64URL.encode(try FleetTests.hex(fixture["message"])),
            signature: Base64URL.encode(try FleetTests.hex(fixture["signature"]))
        )
        let relay = try #require(route.verified()).relay
        #expect(!relay.relays.isEmpty && !relay.route.isEmpty && !relay.identity.isEmpty)
        // Another host's id: its certificate does not hash to it.
        let elsewhere = API.FleetRoute(
            host: String(repeating: "05", count: 32), certificate: route.certificate,
            key: route.key, record: route.record, signature: route.signature
        )
        #expect(elsewhere.verified() == nil)
    }

    @Test func aRecordTheCASignedBecomesTheRelayRecord() throws {
        let host = Host()
        let record = host.record()
        let keys = RelayKeyPair()
        let made = try #require(Self.status([try host.route(record)]).relayRecord(for: host.id.hexString.uppercased(), keys: keys))
        #expect(made.dialKey == keys.dialKey && made.tunnelKey == keys.tunnelKey)
        #expect(made.route == Data(repeating: 0x52, count: 32))
        #expect(made.identity == Base64URL.decode(record.identity))
        #expect(made.relays.map(\.description) == ["https://relay.example.org"])
    }

    @Test func aRouteTheCADidNotSignIsRefused() throws {
        let host = Host()
        let record = host.record()
        let good = try host.route(record)
        // Signed by another key: the carrying computer's, say.
        #expect(try host.route(record, signedBy: P256.Signing.PrivateKey()).verified() == nil)
        // A record for another host, signed by this CA.
        #expect(try host.route(host.record(host: Data(repeating: 5, count: 32))).verified() == nil)
        // The record changed after signing.
        var moved = record
        moved.relays = ["https://evil.example.org"]
        let swapped = API.FleetRoute(
            host: good.host, certificate: good.certificate, key: good.key,
            record: Base64URL.encode(try moved.message()), signature: good.signature
        )
        #expect(swapped.verified() == nil)
        // A key the certificate does not hold.
        let other = P256.Signing.PrivateKey()
        let foreign = API.FleetRoute(
            host: good.host, certificate: good.certificate,
            key: Base64URL.encode(other.publicKey.x963Representation),
            record: good.record, signature: Base64URL.encode(try other.signature(for: try record.message()).rawRepresentation)
        )
        #expect(foreign.verified() == nil)
        // Not linked: no route at all.
        #expect(Self.status([]).relayRecord(for: host.id.hexString, keys: RelayKeyPair()) == nil)
    }

    @Test func onlyANewerRecordReplacesSavedRelays() throws {
        let host = Host()
        let keys = RelayKeyPair()
        let id = host.id.hexString
        let saved = try #require(Self.status([try host.route(host.record(rev: 5))]).relayRecord(for: id, keys: keys))
        #expect(saved.signedRev == 5 && saved.signedMs == 1)
        // The same rev, or an older one a computer serves again, is refused,
        // whatever relays it names.
        for rev: UInt64 in [5, 4, 0] {
            let older = Self.status([try host.route(host.record(rev: rev))])
            #expect(older.relayRecord(for: id, keys: keys, replacing: saved) == nil)
        }
        let newer = try #require(
            Self.status([try host.route(host.record(rev: 6))]).relayRecord(for: id, keys: keys, replacing: saved)
        )
        #expect(newer.signedRev == 6)
        // Relays a computer told the phone itself carry no rev: any checked
        // record replaces them.
        var direct = saved
        direct.signedRev = nil
        #expect(Self.status([try host.route(host.record(rev: 1))]).relayRecord(for: id, keys: keys, replacing: direct) != nil)
        // Kept across launches.
        let decoded = try JSONDecoder().decode(RelayRecord.self, from: JSONEncoder().encode(newer))
        #expect(decoded.signedRev == 6 && decoded.signedMs == 1)
    }

    @Test func aDirectRefreshKeepsTheCheckedRevFloor() throws {
        let host = Host()
        let id = host.id.hexString
        let keys = RelayKeyPair()
        let checked = try #require(Self.status([try host.route(host.record(rev: 5))]).relayRecord(for: id, keys: keys))
        // A direct connection saves a fresh record, which carries no rev.
        var direct = checked
        direct.signedRev = nil
        direct.signedMs = nil
        let saved = direct.keepingSignedRev(of: checked)
        #expect(saved.signedRev == 5)
        // So an older record a lagging computer serves is still refused.
        let older = Self.status([try host.route(host.record(rev: 4))])
        #expect(older.relayRecord(for: id, keys: keys, replacing: saved) == nil)
        // A newer floor wins, and a record with none keeps none.
        var newer = checked
        newer.signedRev = 7
        #expect(newer.keepingSignedRev(of: checked).signedRev == 7)
        #expect(direct.keepingSignedRev(of: nil).signedRev == nil)
    }

    @Test func linkKeysDecode() throws {
        let key = Data(repeating: 4, count: 65)
        let json = #"{"heads":[],"links":["AA"],"link_keys":{"AA":"\#(Base64URL.encode(key))","bb":"!"}}"#
        let status = try JSONDecoder().decode(API.FleetStatus.self, from: Data(json.utf8))
        #expect(status.linkedKeys == ["aa": key])
    }

    @Test func routesDecode() throws {
        let json = #"{"heads":[],"routes":[{"host":"h","certificate":"c","key":"k","record":"r","signature":"s"}]}"#
        let status = try JSONDecoder().decode(API.FleetStatus.self, from: Data(json.utf8))
        #expect(status.routes == [API.FleetRoute(host: "h", certificate: "c", key: "k", record: "r", signature: "s")])
        #expect(status.relay(for: "h") == nil)
        #expect(try JSONDecoder().decode(API.FleetStatus.self, from: Data(#"{"heads":[]}"#.utf8)).routes.isEmpty)
    }
}
