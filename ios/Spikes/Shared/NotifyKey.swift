import CryptoKit
import Foundation
import Security

/// The key sealed notifications are opened with: a Secure Enclave
/// key-agreement key where there is one (its device-bound blob is what the
/// keychain holds), a software key on the simulator. Kept in the App
/// Group's keychain access group so the Notification Service Extension can
/// use the key the app made, and also as a file in the App Group container:
/// on a device the process that draws a Live Activity gets
/// `errSecNotAvailable` from the keychain, and a Secure Enclave key's blob
/// is only a handle this phone's Secure Enclave can use, so a file
/// protected until first unlock holds it as safely.
enum NotifyKey {
    enum Stored {
        case secureEnclave(SecureEnclave.P256.KeyAgreement.PrivateKey)
        case software(P256.KeyAgreement.PrivateKey)

        var publicKey: P256.KeyAgreement.PublicKey {
            switch self {
            case let .secureEnclave(key): key.publicKey
            case let .software(key): key.publicKey
            }
        }

        func agree(_ other: P256.KeyAgreement.PublicKey) throws -> SharedSecret {
            switch self {
            case let .secureEnclave(key): try key.sharedSecretFromKeyAgreement(with: other)
            case let .software(key): try key.sharedSecretFromKeyAgreement(with: other)
            }
        }
    }

    enum Failure: Error {
        case keychain(OSStatus)
        case both(keychain: OSStatus, file: String)
    }

    /// Where `loadAny` found the key, for the probe's report.
    enum Source: String {
        case keychain
        case file
    }

    private static var file: URL? {
        FileManager.default.containerURL(forSecurityApplicationGroupIdentifier: SpikeIDs.appGroup)?
            .appending(path: "notify-key.bin")
    }

    private static func query() -> [String: Any] {
        [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: "so.pen.herdr-spikes",
            kSecAttrAccount as String: SpikeIDs.notifyKeyAccount,
            kSecAttrAccessGroup as String: SpikeIDs.keychainGroup,
        ]
    }

    /// The stored key, or nil before the app made one.
    static func load() throws -> Stored? {
        var query = query()
        query[kSecReturnData as String] = true
        var result: CFTypeRef?
        let status = SecItemCopyMatching(query as CFDictionary, &result)
        guard status != errSecItemNotFound else { return nil }
        guard status == errSecSuccess, let data = result as? Data, let kind = data.first else {
            throw Failure.keychain(status)
        }
        return try decode(kind: kind, body: data.dropFirst())
    }

    /// The key from the keychain, or from the App Group file when the
    /// keychain refuses this process.
    static func loadAny() throws -> (Stored, Source)? {
        let status: OSStatus
        do {
            guard let key = try load() else { return try loadFile().map { ($0, .file) } }
            return (key, .keychain)
        } catch let Failure.keychain(code) {
            status = code
        }
        do {
            return try loadFile().map { ($0, .file) }
        } catch {
            throw Failure.both(keychain: status, file: "\(error)")
        }
    }

    private static func loadFile() throws -> Stored? {
        guard let file, FileManager.default.fileExists(atPath: file.path) else { return nil }
        let data = try Data(contentsOf: file)
        guard let kind = data.first else { return nil }
        return try decode(kind: kind, body: data.dropFirst())
    }

    /// Brings the App Group file in line with the keychain, which the app
    /// can always read: writes it for a key made before the file existed,
    /// and rewrites one left behind with an older key, as when writing it
    /// failed after a new key was made.
    static func mirror() throws {
        guard let file else { return }
        var query = query()
        query[kSecReturnData as String] = true
        var result: CFTypeRef?
        guard SecItemCopyMatching(query as CFDictionary, &result) == errSecSuccess, let data = result as? Data else {
            return
        }
        guard (try? Data(contentsOf: file)) != data else { return }
        try write(data)
    }

    private static func write(_ data: Data) throws {
        guard let file else { return }
        try data.write(to: file, options: [.atomic, .completeFileProtectionUntilFirstUserAuthentication])
    }

    private static func decode(kind: UInt8, body: Data) throws -> Stored {
        kind == 1
            ? .secureEnclave(try SecureEnclave.P256.KeyAgreement.PrivateKey(dataRepresentation: body))
            : .software(try P256.KeyAgreement.PrivateKey(rawRepresentation: body))
    }

    /// Makes and stores a new key, replacing any old one.
    static func create() throws -> Stored {
        let stored: Stored
        let data: Data
        if SecureEnclave.isAvailable {
            let key = try SecureEnclave.P256.KeyAgreement.PrivateKey()
            stored = .secureEnclave(key)
            data = Data([1]) + key.dataRepresentation
        } else {
            let key = P256.KeyAgreement.PrivateKey()
            stored = .software(key)
            data = Data([0]) + key.rawRepresentation
        }
        // Without the old file, a failed write below leaves the widget no
        // key rather than a stale one; `mirror` writes it again later.
        if let file { try? FileManager.default.removeItem(at: file) }
        SecItemDelete(query() as CFDictionary)
        var add = query()
        add[kSecValueData as String] = data
        add[kSecAttrAccessible as String] = kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly
        let status = SecItemAdd(add as CFDictionary, nil)
        guard status == errSecSuccess else { throw Failure.keychain(status) }
        try write(data)
        return stored
    }
}
