import CryptoKit
import Foundation
import HerdrRemoteCore
import Security

/// The key sealed alerts are opened with (`NotificationSeal`): a Secure
/// Enclave key-agreement key where there is one (the keychain holds its
/// device-bound blob), a software key on the simulator. It lives in the
/// App Group's keychain access group, so the Notification Service
/// Extension opens alerts with the key the app registered.
enum NotifyKey {
    static let group = "group.so.pen.herdr-remote"

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

        /// Opens an alert's `sealed` field.
        func open(_ sealed: String) throws -> SealedAlert {
            try SealedAlert.open(sealed, device: publicKey) { try agree($0) }
        }
    }

    enum Failure: Error {
        case keychain(OSStatus)
    }

    private static func query() -> [String: Any] {
        [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: "so.pen.herdr-remote",
            kSecAttrAccount as String: "notify-key",
            kSecAttrAccessGroup as String: group,
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
        let body = data.dropFirst()
        return kind == 1
            ? .secureEnclave(try SecureEnclave.P256.KeyAgreement.PrivateKey(dataRepresentation: body))
            : .software(try P256.KeyAgreement.PrivateKey(rawRepresentation: body))
    }

    /// The stored key, made on first use.
    static func loadOrCreate() throws -> Stored {
        if let stored = try load() { return stored }
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
        var add = query()
        add[kSecValueData as String] = data
        add[kSecAttrAccessible as String] = kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly
        let status = SecItemAdd(add as CFDictionary, nil)
        guard status == errSecSuccess else { throw Failure.keychain(status) }
        return stored
    }
}
