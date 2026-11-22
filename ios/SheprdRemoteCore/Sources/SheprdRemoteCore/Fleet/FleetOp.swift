import CryptoKit
import Foundation

extension Fleet {
    static let version: UInt8 = 1
    static let maxOp = 16 * 1024
    static let maxParents = 16

    /// A signed fleet operation, as `control/fleet/op.rs` encodes it: the
    /// signed bytes, then the 64-byte signature. Its id is the SHA-256 of
    /// the signed bytes alone, so another encoding of one signature names
    /// the same operation.
    public struct Op: Equatable, Sendable {
        public var fleet: Data
        public var signer: Data
        public var seq: UInt64
        public var issuedMs: UInt64
        public var parents: [Data]
        public var body: Body
        public var signature: Data

        public init(fleet: Data, signer: Data, seq: UInt64, issuedMs: UInt64, parents: [Data], body: Body, signature: Data) {
            self.fleet = fleet
            self.signer = signer
            self.seq = seq
            self.issuedMs = issuedMs
            self.parents = parents
            self.body = body
            self.signature = signature
        }

        /// The bytes its signature covers and its id hashes.
        public func signedBytes() throws -> Data {
            guard fleet.count == 32, signer.count == 32, parents.count <= maxParents,
                  parents.allSatisfy({ $0.count == 32 }),
                  parents == parents.sorted(by: { $0.lexicographicallyPrecedes($1) }),
                  Set(parents).count == parents.count
            else { throw FleetError.invalid }
            // Only the `found` that makes a fleet has no parents, and its
            // fleet id is zero.
            let found = if case .found = body { true } else { false }
            guard found == parents.isEmpty, found == (fleet == Data(count: 32)) else { throw FleetError.invalid }
            var writer = FleetWriter()
            writer.raw(Fleet.domain)
            writer.u8(version)
            writer.raw(fleet)
            writer.raw(signer)
            writer.u64(seq)
            writer.u64(issuedMs)
            writer.u8(UInt8(parents.count))
            parents.forEach { writer.raw($0) }
            try body.write(&writer)
            guard writer.bytes.count + 64 <= maxOp else { throw FleetError.bounds }
            return writer.bytes
        }

        public func encode() throws -> Data { try signedBytes() + signature }

        public func id() throws -> Data { Data(SHA256.hash(data: try signedBytes())) }

        /// Whether `key` (SEC1 compressed) signed it.
        public func verify(key: Data) -> Bool {
            guard let key = try? P256.Signing.PublicKey(compressedRepresentation: key),
                  let signature = try? P256.Signing.ECDSASignature(rawRepresentation: signature),
                  let message = try? signedBytes()
            else { return false }
            return key.isValidSignature(signature, for: message)
        }

        /// Signs it as this phone, with its fleet key.
        public mutating func sign(with key: P256.Signing.PrivateKey) throws {
            signature = try key.signature(for: signedBytes()).rawRepresentation
        }

        /// Reads an operation strictly: bytes left over, or another
        /// encoding than this one writes, are refused.
        public static func decode(_ data: Data) throws -> Op {
            guard data.count >= 64, data.count <= maxOp else { throw FleetError.bounds }
            let signed = data.prefix(data.count - 64)
            var reader = FleetReader(signed)
            guard try reader.raw(Fleet.domain.count) == Fleet.domain, try reader.u8() == version else {
                throw FleetError.encoding
            }
            let fleet = try reader.raw(32)
            let signer = try reader.raw(32)
            let seq = try reader.u64()
            let issuedMs = try reader.u64()
            let count = Int(try reader.u8())
            guard count <= maxParents else { throw FleetError.bounds }
            let parents = try (0..<count).map { _ in try reader.raw(32) }
            let body = try Body.read(&reader)
            try reader.finish()
            let op = Op(
                fleet: fleet, signer: signer, seq: seq, issuedMs: issuedMs, parents: parents,
                body: body, signature: Data(data.suffix(64))
            )
            guard try op.signedBytes() == signed else { throw FleetError.encoding }
            return op
        }
    }
}
