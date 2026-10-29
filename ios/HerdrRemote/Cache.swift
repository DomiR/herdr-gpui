import CryptoKit
import Foundation
import HerdrRemoteCore

/// The last state of every computer and of recently opened chats, so the
/// app draws at once on launch and a chat resumes from its cursor rather
/// than a snapshot. Files use complete data protection: readable only
/// while the phone is unlocked.
struct Cache: Sendable {
    let folder: URL

    /// Entries kept per cached chat, newest last.
    static let entriesKept = 200
    /// Chats kept per computer; the least recently opened go first.
    static let chatsKept = 24

    struct CachedChat: Codable, Sendable {
        var cursor: Cursor?
        var transcript: Transcript
    }

    static let shared: Cache = {
        let base = FileManager.default.containerURL(forSecurityApplicationGroupIdentifier: "group.so.pen.herdr.remote")
            ?? URL.applicationSupportDirectory
        return Cache(folder: base.appending(path: "Cache", directoryHint: .isDirectory))
    }()

    private func computerFolder(_ device: String) -> URL {
        folder.appending(path: Self.name(device), directoryHint: .isDirectory)
    }

    /// A file name for an id the daemon chose: its SHA-256 in hex, so no
    /// id (`..`, a slash) can name a path outside the cache.
    static func name(_ id: String) -> String {
        SHA256.hash(data: Data(id.utf8)).map { String(format: "%02x", $0) }.joined()
    }

    func home(_ device: String) -> Home? {
        read(Home.self, at: computerFolder(device).appending(path: "home.json"))
    }

    func save(home: Home, device: String) {
        write(home, to: computerFolder(device).appending(path: "home.json"))
    }

    func chat(_ device: String, pane: String) -> CachedChat? {
        read(CachedChat.self, at: chatFile(device, pane))
    }

    func save(chat transcript: Transcript, cursor: Cursor?, device: String, pane: String) {
        var kept = transcript
        if kept.entries.count > Self.entriesKept {
            kept.entries.removeFirst(kept.entries.count - Self.entriesKept)
            kept.truncated = true
        }
        let file = chatFile(device, pane)
        let chats = computerFolder(device).appending(path: "chats", directoryHint: .isDirectory)
        Task.detached(priority: .utility) {
            Self.store(CachedChat(cursor: cursor, transcript: kept), to: file)
            Self.evict(in: chats, keeping: Self.chatsKept)
        }
    }

    /// Forgets one computer, as unpairing does, or everything.
    func clear(_ device: String? = nil) {
        try? FileManager.default.removeItem(at: device.map(computerFolder) ?? folder)
    }

    /// Bytes on disk, for Settings.
    func size() -> Int {
        let files = FileManager.default.enumerator(at: folder, includingPropertiesForKeys: [.fileSizeKey])
        return (files?.allObjects as? [URL] ?? []).reduce(0) {
            $0 + ((try? $1.resourceValues(forKeys: [.fileSizeKey]).fileSize) ?? 0)
        }
    }

    private func chatFile(_ device: String, _ pane: String) -> URL {
        computerFolder(device).appending(path: "chats/\(Self.name(pane)).json")
    }

    private func read<Value: Decodable>(_ type: Value.Type, at url: URL) -> Value? {
        guard let data = try? Data(contentsOf: url) else { return nil }
        return try? JSONDecoder().decode(type, from: data)
    }

    private func write<Value: Encodable & Sendable>(_ value: Value, to url: URL) {
        Task.detached(priority: .utility) { Self.store(value, to: url) }
    }

    private static func store<Value: Encodable>(_ value: Value, to url: URL) {
        guard let data = try? JSONEncoder().encode(value) else { return }
        try? FileManager.default.createDirectory(at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
        try? data.write(to: url, options: [.atomic, .completeFileProtection])
    }

    private static func evict(in folder: URL, keeping limit: Int) {
        let keys: [URLResourceKey] = [.contentModificationDateKey]
        guard let files = try? FileManager.default.contentsOfDirectory(at: folder, includingPropertiesForKeys: keys),
              files.count > limit
        else { return }
        let dated = files.map { ($0, (try? $0.resourceValues(forKeys: Set(keys)).contentModificationDate) ?? .distantPast) }
        for (file, _) in dated.sorted(by: { $0.1 < $1.1 }).prefix(files.count - limit) {
            try? FileManager.default.removeItem(at: file)
        }
    }
}
