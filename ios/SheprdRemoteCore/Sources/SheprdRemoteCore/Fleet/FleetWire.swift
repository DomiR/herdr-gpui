import Foundation

/// The fleet's canonical bytes, as `crates/sheprd-remote-control/src/control/fleet/wire.rs`
/// writes them: fixed fields as they are, big-endian; variable ones after
/// their length as a big-endian `UInt16`; lists after their count;
/// optional fields after a `0` or `1` byte. Reading is strict.
public enum FleetError: Error, Equatable, Sendable {
    case bounds
    case encoding
    case invalid
    case signature
}

/// Writes canonical bytes.
struct FleetWriter {
    private(set) var bytes = Data()

    mutating func raw(_ data: Data) { bytes.append(data) }
    mutating func raw(_ data: [UInt8]) { bytes.append(contentsOf: data) }
    mutating func u8(_ value: UInt8) { bytes.append(value) }
    mutating func u16(_ value: UInt16) { bytes.append(contentsOf: [UInt8(value >> 8), UInt8(value & 0xff)]) }

    mutating func u64(_ value: UInt64) {
        bytes.append(contentsOf: (0..<8).reversed().map { UInt8((value >> ($0 * 8)) & 0xff) })
    }

    mutating func bool(_ value: Bool) { u8(value ? 1 : 0) }

    mutating func count(_ count: Int) throws {
        guard let value = UInt16(exactly: count) else { throw FleetError.bounds }
        u16(value)
    }

    mutating func bytes(_ data: Data) throws {
        try count(data.count)
        raw(data)
    }

    mutating func text(_ text: String) throws { try bytes(Data(text.utf8)) }

    mutating func texts(_ texts: [String]) throws {
        try count(texts.count)
        for text in texts { try self.text(text) }
    }

    mutating func option<T>(_ value: T?, _ write: (inout FleetWriter, T) throws -> Void) rethrows {
        guard let value else {
            u8(0)
            return
        }
        u8(1)
        try write(&self, value)
    }
}

/// Reads canonical bytes.
struct FleetReader {
    private let bytes: [UInt8]
    private var at = 0

    init(_ data: Data) { bytes = [UInt8](data) }

    var remaining: Int { bytes.count - at }

    func finish() throws {
        guard remaining == 0 else { throw FleetError.encoding }
    }

    mutating func raw(_ length: Int) throws -> Data {
        guard length >= 0, remaining >= length else { throw FleetError.encoding }
        defer { at += length }
        return Data(bytes[at..<at + length])
    }

    mutating func u8() throws -> UInt8 { try raw(1)[0] }

    mutating func u16() throws -> UInt16 {
        let data = [UInt8](try raw(2))
        return UInt16(data[0]) << 8 | UInt16(data[1])
    }

    mutating func u64() throws -> UInt64 {
        try raw(8).reduce(0) { $0 << 8 | UInt64($1) }
    }

    mutating func bool() throws -> Bool {
        switch try u8() {
        case 0: false
        case 1: true
        default: throw FleetError.encoding
        }
    }

    mutating func count(max: Int) throws -> Int {
        let count = Int(try u16())
        guard count <= max else { throw FleetError.bounds }
        return count
    }

    mutating func bytes(max: Int) throws -> Data { try raw(try count(max: max)) }

    mutating func text(max: Int) throws -> String {
        guard let text = String(data: try bytes(max: max), encoding: .utf8) else { throw FleetError.encoding }
        return text
    }

    mutating func texts(count: Int, max: Int) throws -> [String] {
        try (0..<self.count(max: count)).map { _ in try text(max: max) }
    }

    mutating func option<T>(_ read: (inout FleetReader) throws -> T) throws -> T? {
        try bool() ? read(&self) : nil
    }
}
