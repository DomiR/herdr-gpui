import CryptoKit
import Foundation
import SheprdRemoteCore
import Security

/// The keys sealed alerts are opened with (`NotificationSeal`), one per
/// paired computer: a Secure Enclave key-agreement key where there is one
/// (the keychain holds its device-bound blob), a software key on the
/// simulator. An alert counts as from the computer whose key opens it,
/// never the computer its text names, so one paired computer cannot send
/// an alert whose actions reach another. They live in the App Group's
/// keychain group, so the Notification Service Extension opens alerts
/// with the keys the app registered.
///
/// While the push relay is on, they are also kept in a file in the App
/// Group container, protected until first unlock: on a device the widget
/// extension that draws a Live Activity gets `errSecNotAvailable` from the
/// keychain (`docs/ios-spikes.md`), and it opens the activity content a
/// computer sealed for the push relay. A Secure Enclave key's blob is a
/// handle only this phone's Secure Enclave can use, so the file holds
/// nothing a keychain item would protect better; the simulator's software
/// keys are the exception, and are test keys.
///
/// One more key is this phone's own (`own`), kept in its own keychain item
/// and never given to a computer: the phone seals the test pushes it sends
/// itself through the push relay to it (`SelfTestPush`). It is never among
/// the computers' keys, so nothing it opens counts as from a computer, and
/// what it opens is taken only as a self test, which has no actions.
enum NotifyKey {
    static let group = "group.so.pen.herdr.remote"

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

        /// The public half as `POST /api/apns` takes it.
        var registered: String { Base64URL.encode(publicKey.x963Representation) }

        fileprivate var data: Data {
            switch self {
            case let .secureEnclave(key): Data([1]) + key.dataRepresentation
            case let .software(key): Data([0]) + key.rawRepresentation
            }
        }

        fileprivate init(_ data: Data) throws {
            let body = data.dropFirst()
            self = data.first == 1
                ? .secureEnclave(try SecureEnclave.P256.KeyAgreement.PrivateKey(dataRepresentation: body))
                : .software(try P256.KeyAgreement.PrivateKey(rawRepresentation: body))
        }
    }

    enum Failure: Error {
        case keychain(OSStatus)
    }

    /// An opened alert and the computer whose key opened it.
    struct Opened {
        let computer: Fingerprint
        let alert: SealedAlert
    }

    /// Opens an alert's `sealed` field with whichever computer's key fits,
    /// and only when the alert names that same computer.
    static func open(_ sealed: String) throws -> Opened? {
        for (computer, key) in try loadAll() {
            guard let alert = try? SealedAlert.open(sealed, device: key.publicKey, agree: { try key.agree($0) }) else { continue }
            return Fingerprint(alert.computer) == computer ? Opened(computer: computer, alert: alert) : nil
        }
        return nil
    }

    /// Opens a self test (`SelfTestPush`) with this phone's own key. `nil`
    /// when there is no such key yet, or it does not open as a self test.
    static func openSelfTest(_ sealed: String) throws -> SelfTestPush.Opened? {
        guard let key = try loadOwn() else { return nil }
        return try? SelfTestPush.open(sealed, device: key.publicKey, agree: { try key.agree($0) })
    }

    /// This phone's own key, which self tests are sealed to, made on first use.
    static func own() throws -> Stored {
        if let key = try loadOwn() { return key }
        let key = try newKey()
        try write(key.data, to: query(ownAccount))
        return key
    }

    private static func loadOwn() throws -> Stored? {
        try read(query(ownAccount)).map(Stored.init)
    }

    private static func newKey() throws -> Stored {
        SecureEnclave.isAvailable
            ? .secureEnclave(try SecureEnclave.P256.KeyAgreement.PrivateKey())
            : .software(P256.KeyAgreement.PrivateKey())
    }

    /// The key alerts from `computer` are sealed to, made on first use.
    static func loadOrCreate(for computer: Fingerprint) throws -> Stored {
        var keys = try loadAll()
        if let key = keys[computer] { return key }
        let key = try newKey()
        keys[computer] = key
        try save(keys)
        return key
    }

    /// Every computer's key, as sealed Live Activity content is opened:
    /// from the keychain, or from the App Group file where the keychain
    /// refuses this process. None at all means the widget shows its cover.
    static func activityKeys() -> [SealedChatActivity.Key] {
        let keys: [Fingerprint: Stored]
        do {
            keys = try loadAll()
        } catch Failure.keychain {
            keys = (try? loadFile()) ?? [:]
        } catch {
            keys = [:]
        }
        return keys.map { computer, key in
            SealedChatActivity.Key(computer: computer, device: key.publicKey, agree: key.agree)
        }
    }

    /// Forgets an unpaired computer's key, so its alerts no longer open.
    static func forget(_ computer: Fingerprint) throws {
        var keys = try loadAll()
        guard keys.removeValue(forKey: computer) != nil else { return }
        try save(keys)
    }

    /// The computers' keys' item.
    private static let account = "notify-keys"
    /// This phone's own key's item, apart, so it is never a computer's.
    private static let ownAccount = "notify-own-key"

    private static func query(_ account: String = account) -> [String: Any] {
        [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: "so.pen.herdr.remote",
            kSecAttrAccount as String: account,
            kSecAttrAccessGroup as String: group,
        ]
    }

    static func loadAll() throws -> [Fingerprint: Stored] {
        try read(query()).map(decode) ?? [:]
    }

    private static func read(_ query: [String: Any]) throws -> Data? {
        var query = query
        query[kSecReturnData as String] = true
        var result: CFTypeRef?
        let status = SecItemCopyMatching(query as CFDictionary, &result)
        guard status != errSecItemNotFound else { return nil }
        guard status == errSecSuccess, let data = result as? Data else { throw Failure.keychain(status) }
        return data
    }

    private static var file: URL? {
        FileManager.default.containerURL(forSecurityApplicationGroupIdentifier: group)?
            .appending(path: "notify-keys.json")
    }

    private static func decode(_ data: Data) throws -> [Fingerprint: Stored] {
        let stored = try JSONDecoder().decode([String: Data].self, from: data)
        var keys: [Fingerprint: Stored] = [:]
        for (hex, blob) in stored {
            if let computer = Fingerprint(hex) { keys[computer] = try Stored(blob) }
        }
        return keys
    }

    private static func loadFile() throws -> [Fingerprint: Stored] {
        guard let file, FileManager.default.fileExists(atPath: file.path) else { return [:] }
        return try decode(Data(contentsOf: file))
    }

    /// Brings the App Group file in line with the keychain, which the app
    /// can always read: written while the push relay is on, removed while
    /// it is off. The app calls it at launch and when the setting changes.
    static func mirror() throws {
        guard let file else { return }
        guard PushRelaySetting.isOn else {
            if FileManager.default.fileExists(atPath: file.path) { try FileManager.default.removeItem(at: file) }
            return
        }
        try writeFile(encoded(try loadAll()))
    }

    private static func writeFile(_ data: Data) throws {
        guard let file, PushRelaySetting.isOn else { return }
        guard (try? Data(contentsOf: file)) != data else { return }
        try data.write(to: file, options: [.atomic, .completeFileProtectionUntilFirstUserAuthentication])
    }

    private static func encoded(_ keys: [Fingerprint: Stored]) throws -> Data {
        let encoder = JSONEncoder()
        encoder.outputFormatting = .sortedKeys
        return try encoder.encode(Dictionary(uniqueKeysWithValues: keys.map { ($0.key.hex, $0.value.data) }))
    }

    private static func save(_ keys: [Fingerprint: Stored]) throws {
        let data = try encoded(keys)
        try write(data, to: query())
        try writeFile(data)
    }

    private static func write(_ data: Data, to query: [String: Any]) throws {
        let attributes: [String: Any] = [
            kSecValueData as String: data,
            kSecAttrAccessible as String: kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly,
        ]
        let status = SecItemUpdate(query as CFDictionary, attributes as CFDictionary)
        if status == errSecItemNotFound {
            let added = SecItemAdd(query.merging(attributes) { $1 } as CFDictionary, nil)
            guard added == errSecSuccess else { throw Failure.keychain(added) }
            return
        }
        guard status == errSecSuccess else { throw Failure.keychain(status) }
    }
}
