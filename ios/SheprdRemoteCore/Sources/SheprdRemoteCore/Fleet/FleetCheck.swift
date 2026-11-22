import CryptoKit
import Foundation

/// The rules the daemon's `body.rs` checks every field against, so the
/// Swift copy refuses what the daemon refuses
/// (`Fixtures/fleet/invalid.json`).
extension Fleet {
    /// A valid P-256 point, SEC1 as the daemon reads it: compressed,
    /// compact, or uncompressed.
    static func isPoint(_ key: Data) -> Bool {
        switch (key.count, key.first) {
        case (33, 0x02), (33, 0x03):
            (try? P256.Signing.PublicKey(compressedRepresentation: key)) != nil
        case (33, 0x05):
            (try? P256.Signing.PublicKey(compactRepresentation: key.dropFirst())) != nil
        case (65, 0x04):
            (try? P256.Signing.PublicKey(x963Representation: key)) != nil
        default:
            false
        }
    }

    static func isControl(_ scalar: Unicode.Scalar) -> Bool {
        scalar.properties.generalCategory == .control
    }

    /// A name a person reads: not blank, no control characters.
    static func checkName(_ text: String, max: Int) throws {
        guard text.utf8.count <= max,
              !text.unicodeScalars.allSatisfy(\.properties.isWhitespace),
              !text.unicodeScalars.contains(where: isControl)
        else { throw FleetError.invalid }
    }

    /// A field without spaces or control characters.
    static func checkField(_ text: String, max: Int) throws {
        guard !text.isEmpty, text.utf8.count <= max,
              !text.unicodeScalars.contains(where: { isControl($0) || $0 == " " })
        else { throw FleetError.invalid }
    }

    /// `TYPE BASE64`: two fields of printable ASCII, so it can go on an
    /// `authorized_keys` or `known_hosts` line as it is.
    static func checkSSHKey(_ text: String, max: Int) throws {
        let fields = text.utf8.split(separator: UInt8(ascii: " "), omittingEmptySubsequences: false)
        guard text.utf8.count <= max, fields.count == 2,
              let kind = fields.first, let blob = fields.last,
              !kind.isEmpty, !blob.isEmpty,
              kind.allSatisfy({ isAlphanumeric($0) || "-@.".utf8.contains($0) }),
              blob.allSatisfy({ isAlphanumeric($0) || "+/=".utf8.contains($0) })
        else { throw FleetError.invalid }
    }

    static func isAlphanumeric(_ byte: UInt8) -> Bool {
        (0x30...0x39).contains(byte) || (0x41...0x5a).contains(byte) || (0x61...0x7a).contains(byte)
    }
}
