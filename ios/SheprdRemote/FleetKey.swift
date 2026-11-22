import CryptoKit
import Foundation
import LocalAuthentication
import SheprdRemoteCore
import Security

/// The fleet's signed log and its operations (`SheprdRemoteCore.Fleet`),
/// named apart from this app's `Fleet` of paired computers' links.
typealias FleetLog = SheprdRemoteCore.Fleet

/// This phone's fleet key (`docs/relay-plan.md`, Root of trust): made once
/// when it joins, kept in the Secure Enclave, and usable only with user
/// presence, so every fleet operation the phone signs asks for Face ID or
/// the passcode. It never leaves the phone; a new one means joining again.
enum FleetKey {
    case secureEnclave(SecureEnclave.P256.Signing.PrivateKey)
    /// The simulator only, which has no Secure Enclave: a plain software
    /// key, with no user presence. A real phone never makes one.
    case simulator(P256.Signing.PrivateKey)

    enum Failure: Error, LocalizedError {
        case accessControl
        case noSecureEnclave

        var errorDescription: String? {
            switch self {
            case .accessControl: "Could not set up the fleet key's access control."
            case .noSecureEnclave: "This device has no Secure Enclave to keep a fleet key in."
            }
        }
    }

    /// A new key for joining a fleet.
    static func create() throws -> FleetKey {
        #if targetEnvironment(simulator)
        return .simulator(P256.Signing.PrivateKey())
        #else
        guard SecureEnclave.isAvailable else { throw Failure.noSecureEnclave }
        guard let access = SecAccessControlCreateWithFlags(
            nil,
            // Signing needs the phone unlocked and the person there: the
            // fleet signs only what someone asked for, never in the
            // background.
            kSecAttrAccessibleWhenUnlockedThisDeviceOnly,
            [.privateKeyUsage, .userPresence],
            nil
        ) else { throw Failure.accessControl }
        return .secureEnclave(try SecureEnclave.P256.Signing.PrivateKey(accessControl: access))
        #endif
    }

    /// The key a saved membership names. `context`, once evaluated, lets a
    /// batch of signatures (one `Link` per host) share one Face ID.
    init(kind: Computer.KeyKind, data: Data, context: LAContext? = nil) throws {
        switch kind {
        case .secureEnclave:
            self = .secureEnclave(try SecureEnclave.P256.Signing.PrivateKey(
                dataRepresentation: data, authenticationContext: context
            ))
        case .software:
            #if targetEnvironment(simulator)
            self = .simulator(try P256.Signing.PrivateKey(rawRepresentation: data))
            #else
            throw Failure.noSecureEnclave
            #endif
        }
    }

    var kind: Computer.KeyKind {
        switch self {
        case .secureEnclave: .secureEnclave
        case .simulator: .software
        }
    }

    /// What a membership record keeps to rebuild the key: the Secure
    /// Enclave's opaque, device-bound blob.
    var data: Data {
        switch self {
        case let .secureEnclave(key): key.dataRepresentation
        case let .simulator(key): key.rawRepresentation
        }
    }

    /// SEC1 compressed, as a fleet bundle carries it.
    var publicKey: Data {
        switch self {
        case let .secureEnclave(key): key.publicKey.compressedRepresentation
        case let .simulator(key): key.publicKey.compressedRepresentation
        }
    }

    /// The signer `FleetClient` uses. Each signature asks for user presence
    /// unless the key was opened with an evaluated context.
    var signer: FleetLog.Signer {
        switch self {
        case let .secureEnclave(key):
            FleetLog.Signer(publicKey: publicKey) { try key.signature(for: $0).rawRepresentation }
        case let .simulator(key):
            FleetLog.Signer(publicKey: publicKey) { try key.signature(for: $0).rawRepresentation }
        }
    }
}
