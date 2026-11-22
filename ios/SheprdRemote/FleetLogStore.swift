import Foundation

/// Where this phone keeps its own copy of the fleet log: one file in the
/// app group's container, outside the cache Settings empties, written
/// atomically with complete file protection, so it is unreadable while
/// the phone is locked (`docs/relay-plan.md`, Storage and logs). The bytes
/// are only ever read through the fleet library, which checks every
/// operation again, so a changed file computes nothing a signature does
/// not back.
struct FleetLogStore {
    /// The largest file read: the library's own bound on a saved log
    /// (`saved::MAX_SAVED`) is below it.
    static let maxBytes = 96 * 1024 * 1024

    let file: URL

    static let shared = FleetLogStore(
        file: Cache.base.appending(path: "Fleet", directoryHint: .isDirectory).appending(path: "log.bin")
    )

    /// What reading the saved copy came to.
    enum Saved: Equatable {
        /// No copy was ever saved.
        case none
        case bytes(Data)
        /// A copy is there but cannot be read now: the phone is locked, or
        /// it is too large to be one. It must not be replaced by a copy
        /// rebuilt from a computer, which would lack this phone's own
        /// operations not yet handed over.
        case unavailable
    }

    func read() -> Saved {
        guard FileManager.default.fileExists(atPath: file.path) else { return .none }
        guard let size = try? file.resourceValues(forKeys: [.fileSizeKey]).fileSize, size <= Self.maxBytes,
              let bytes = try? Data(contentsOf: file)
        else { return .unavailable }
        return .bytes(bytes)
    }

    func write(_ log: Data) throws {
        guard log.count <= Self.maxBytes else { throw CocoaError(.fileWriteOutOfSpace) }
        try FileManager.default.createDirectory(at: file.deletingLastPathComponent(), withIntermediateDirectories: true)
        try log.write(to: file, options: [.atomic, .completeFileProtection])
    }

    func delete() {
        try? FileManager.default.removeItem(at: file)
    }
}
